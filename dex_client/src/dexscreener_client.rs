use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use thiserror::Error;
use tracing::{debug, error, info, warn};
// ChromiumOxide imports for scraping
use chromiumoxide::browser::{Browser, BrowserConfig};
use chromiumoxide::page::Page;
use futures::StreamExt;
use rand::Rng;

#[derive(Error, Debug)]
pub enum DexScreenerError {
    #[error("HTTP request failed: {0}")]
    HttpError(#[from] reqwest::Error),
    #[error("JSON parsing error: {0}")]
    JsonError(#[from] serde_json::Error),
    #[error("API error: {status} - {message}")]
    ApiError { status: u16, message: String },
    #[error("Rate limit exceeded")]
    RateLimitExceeded,
    #[error("No data available")]
    NoDataAvailable,
    #[error("Browser automation error: {0}")]
    BrowserError(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexScreenerBoostedToken {
    #[serde(rename = "chainId")]
    pub chain_id: String,
    #[serde(rename = "tokenAddress")]
    pub token_address: String,
    // Only keeping essential fields for token identification
    pub description: Option<String>,
}

// Simplified response - just array of tokens
pub type DexScreenerBoostedResponse = Vec<DexScreenerBoostedToken>;

/// DexScreener trending token (compatible with BirdEye TrendingToken structure)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DexScreenerTrendingToken {
    pub address: String,
    pub symbol: String,
    pub name: String,
    pub decimals: Option<u8>,
    pub price: f64,
    #[serde(rename = "price24hChangePercent")]
    pub price_change_24h: Option<f64>,
    #[serde(rename = "volume24hUSD")]
    pub volume_24h: Option<f64>,
    #[serde(rename = "volume24hChangePercent")]
    pub volume_change_24h: Option<f64>,
    pub liquidity: Option<f64>,
    pub fdv: Option<f64>,
    pub marketcap: Option<f64>,
    pub rank: Option<u32>,
    #[serde(rename = "logoURI")]
    pub logo_uri: Option<String>,
    #[serde(rename = "txns24h")]
    pub txns_24h: Option<u32>,
    #[serde(rename = "lastTradeUnixTime")]
    pub last_trade_unix_time: Option<i64>,
    // DexScreener specific fields
    pub chain_id: String,
    pub pair_address: Option<String>,
}

/// Configuration for DexScreener client
#[derive(Debug, Clone)]
pub struct DexScreenerConfig {
    pub api_base_url: String,
    pub request_timeout_seconds: u64,
    pub rate_limit_delay_ms: u64, // 60 requests per minute = 1000ms delay
    pub max_retries: u32,
    pub enabled: bool,
    // Browser automation settings
    pub chrome_executable_path: Option<String>,
    pub headless_mode: bool,
    pub anti_detection_enabled: bool,
}

impl Default for DexScreenerConfig {
    fn default() -> Self {
        Self {
            api_base_url: "https://api.dexscreener.com".to_string(),
            request_timeout_seconds: 30,
            rate_limit_delay_ms: 1000, // 1 request per second for 60/min limit
            max_retries: 3,
            enabled: true,
            chrome_executable_path: None,
            headless_mode: false, // Changed to false for GUI debugging
            anti_detection_enabled: true,
        }
    }
}

/// DexScreener API client for fetching boosted tokens and scraping trending data
pub struct DexScreenerClient {
    client: Client,
    config: DexScreenerConfig,
    browser: Option<Browser>,
}

impl DexScreenerClient {
    /// Create a new DexScreener client
    pub fn new(config: DexScreenerConfig) -> Result<Self> {
        let client = Client::builder()
            .timeout(Duration::from_secs(config.request_timeout_seconds))
            .build()?;

        Ok(Self { 
            client, 
            config,
            browser: None,
        })
    }

    /// Initialize browser for scraping (lazy initialization)
    async fn ensure_browser(&mut self) -> Result<&Browser, DexScreenerError> {
        if self.browser.is_none() {
            info!("🔧 Initializing Chrome browser for DexScreener scraping...");
            
            let mut browser_config = BrowserConfig::builder();
            
            // Set Chrome executable path if provided
            if let Some(ref chrome_path) = self.config.chrome_executable_path {
                browser_config = browser_config.chrome_executable(chrome_path);
            }
            
            // Configure headless mode
            if !self.config.headless_mode {
                browser_config = browser_config.with_head();
            }
            
            // IMPORTANT: Set user data directory BEFORE adding stealth args to prevent conflicts
            let profile_dir = {
                let mut rng = rand::thread_rng();
                let random_id: u32 = rng.gen();
                format!("/tmp/chrome-profile-{}", random_id)
            };
            browser_config = browser_config.user_data_dir(&profile_dir);
            
            // Add comprehensive stealth arguments (but exclude user-data-dir since we set it above)
            let stealth_args = self.get_stealth_chrome_args_without_user_data_dir();
            browser_config = browser_config.args(stealth_args);
            
            let (browser, mut handler) = Browser::launch(browser_config.build()
                .map_err(|e| DexScreenerError::BrowserError(format!("Browser config error: {}", e)))?)
                .await
                .map_err(|e| DexScreenerError::BrowserError(format!("Failed to launch browser: {}", e)))?;
            
            // Start handler task
            tokio::spawn(async move {
                while let Some(h) = handler.next().await {
                    if h.is_err() {
                        break;
                    }
                }
            });
            
            self.browser = Some(browser);
            info!("✅ Chrome browser initialized successfully");
        }
        
        Ok(self.browser.as_ref().unwrap())
    }

    /// Check if DexScreener client is enabled
    pub fn is_enabled(&self) -> bool {
        self.config.enabled
    }

    /// Get the latest boosted tokens
    pub async fn get_latest_boosted_tokens(&self) -> Result<Vec<DexScreenerBoostedToken>, DexScreenerError> {
        if !self.config.enabled {
            return Ok(vec![]);
        }

        let url = format!("{}/token-boosts/latest/v1", self.config.api_base_url);
        debug!("🔍 Fetching latest boosted tokens from: {}", url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(DexScreenerError::ApiError { status, message });
        }

        // DexScreener API returns an array of boosted tokens
        let boosted_tokens: Vec<DexScreenerBoostedToken> = response.json().await?;
        
        // Return all tokens without chain filtering to support multichain
        info!("📊 Retrieved {} latest boosted tokens from DexScreener (all chains)", boosted_tokens.len());
        
        if self.config.rate_limit_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.config.rate_limit_delay_ms)).await;
        }

        Ok(boosted_tokens)
    }

    /// Get the top boosted tokens (most active boosts)
    pub async fn get_top_boosted_tokens(&self) -> Result<Vec<DexScreenerBoostedToken>, DexScreenerError> {
        if !self.config.enabled {
            return Ok(vec![]);
        }

        let url = format!("{}/token-boosts/top/v1", self.config.api_base_url);
        debug!("🔍 Fetching top boosted tokens from: {}", url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response.text().await.unwrap_or_else(|_| "Unknown error".to_string());
            return Err(DexScreenerError::ApiError { status, message });
        }

        // DexScreener API returns an array of boosted tokens
        let boosted_tokens: Vec<DexScreenerBoostedToken> = response.json().await?;
        
        // Return all tokens without chain filtering to support multichain
        info!("📊 Retrieved {} top boosted tokens from DexScreener (all chains)", boosted_tokens.len());
        
        if self.config.rate_limit_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.config.rate_limit_delay_ms)).await;
        }

        Ok(boosted_tokens)
    }

    /// Get both latest and top boosted tokens in a single call
    pub async fn get_all_boosted_tokens(&self) -> Result<(Vec<DexScreenerBoostedToken>, Vec<DexScreenerBoostedToken>), DexScreenerError> {
        if !self.config.enabled {
            return Ok((vec![], vec![]));
        }

        debug!("🔍 Fetching all boosted tokens from DexScreener");

        let latest_tokens = self.get_latest_boosted_tokens().await?;
        let top_tokens = self.get_top_boosted_tokens().await?;

        debug!("✅ Retrieved {} latest + {} top boosted tokens", latest_tokens.len(), top_tokens.len());

        Ok((latest_tokens, top_tokens))
    }

    /// Extract unique token addresses from boosted tokens
    pub fn extract_token_addresses(&self, boosted_tokens: &[DexScreenerBoostedToken]) -> Vec<String> {
        let mut addresses: Vec<String> = boosted_tokens
            .iter()
            .map(|token| token.token_address.clone())
            .collect();

        // Remove duplicates and sort
        addresses.sort();
        addresses.dedup();

        debug!("📋 Extracted {} unique token addresses from boosted tokens", addresses.len());
        addresses
    }

    /// Get only the token addresses from boosted tokens (convenience method)
    pub fn get_token_addresses(&self, boosted_tokens: &[DexScreenerBoostedToken]) -> Vec<String> {
        boosted_tokens.iter().map(|token| token.token_address.clone()).collect()
    }

    /// Get configuration
    pub fn get_config(&self) -> &DexScreenerConfig {
        &self.config
    }
    
    /// Get comprehensive stealth Chrome arguments (mirrored from working PoC)
    #[allow(dead_code)]
    fn get_stealth_chrome_args(&self) -> Vec<String> {
        if !self.config.anti_detection_enabled {
            return vec![];
        }
        
        // Generate random profile directory (CRITICAL: prevents conflicts)
        let mut rng = rand::thread_rng();
        let random_id: u32 = rng.gen();
        let profile_dir = format!("/tmp/chrome-profile-{}", random_id);
        
        vec![
            // Core anti-detection flags (from PoC)
            "--disable-blink-features=AutomationControlled".to_string(),
            format!("--user-data-dir={}", profile_dir),
            
            // Exclude automation switches (from PoC)
            "--exclude-switches=enable-automation".to_string(),
            "--disable-infobars".to_string(),
            
            // Disable security features that might interfere (from PoC)
            "--disable-web-security".to_string(),
            "--disable-features=IsolateOrigins,site-per-process".to_string(),
            "--disable-site-isolation-trials".to_string(),
            "--allow-running-insecure-content".to_string(),
            
            // Window and display settings (from PoC)
            "--window-size=1920,1080".to_string(),
            "--start-maximized".to_string(),
            "--force-device-scale-factor=1".to_string(),
            
            // Performance and stability flags (from PoC)
            "--no-sandbox".to_string(),
            "--disable-setuid-sandbox".to_string(),
            "--disable-dev-shm-usage".to_string(),
            "--disable-accelerated-2d-canvas".to_string(),
            "--no-first-run".to_string(),
            "--no-zygote".to_string(),
            "--disable-gpu".to_string(),
            "--disable-background-timer-throttling".to_string(),
            "--disable-backgrounding-occluded-windows".to_string(),
            "--disable-renderer-backgrounding".to_string(),
            "--disable-features=TranslateUI".to_string(),
            "--disable-ipc-flooding-protection".to_string(),
            
            // Additional stealth flags (from PoC)
            "--no-default-browser-check".to_string(),
            "--disable-hang-monitor".to_string(),
            "--disable-prompt-on-repost".to_string(),
            "--disable-sync".to_string(),
            "--disable-domain-reliability".to_string(),
            "--disable-client-side-phishing-detection".to_string(),
            "--disable-component-update".to_string(),
            "--disable-default-apps".to_string(),
            "--disable-extensions".to_string(),
            "--disable-features=ChromeWhatsNewUI".to_string(),
            "--disable-features=ImprovedCookieControls".to_string(),
            "--disable-features=LazyFrameLoading".to_string(),
            "--disable-features=GlobalMediaControls".to_string(),
            "--disable-features=DestroyProfileOnBrowserClose".to_string(),
            "--disable-features=MediaRouter".to_string(),
            "--disable-features=DialMediaRouteProvider".to_string(),
            "--disable-features=AcceptCHFrame".to_string(),
            "--disable-features=AutoExpandDetailsElement".to_string(),
            "--disable-features=CertificateTransparencyComponentUpdater".to_string(),
            "--disable-features=AvoidUnnecessaryBeforeUnloadCheckSync".to_string(),
            "--disable-features=Translate".to_string(),
            
            // Memory optimization (from PoC)
            "--memory-pressure-off".to_string(),
            "--max_old_space_size=4096".to_string(),
            
            // Enable useful features (from PoC)
            "--enable-features=NetworkService,NetworkServiceInProcess".to_string(),
            "--enable-automation".to_string(), // Counterintuitively helps with stability
            "--disable-blink-features".to_string(),
            "--disable-popup-blocking".to_string(),
            "--disable-notifications".to_string(),
        ]
    }

    /// Get comprehensive Chrome stealth arguments without user-data-dir (since that's set via BrowserConfig)
    fn get_stealth_chrome_args_without_user_data_dir(&self) -> Vec<String> {
        vec![
            // Core anti-detection flags (from PoC)
            "--disable-blink-features=AutomationControlled".to_string(),
            // NOTE: --user-data-dir is set via BrowserConfig.user_data_dir() to avoid conflicts
            
            // Exclude automation switches (from PoC)
            "--exclude-switches=enable-automation".to_string(),
            "--disable-infobars".to_string(),
            "--disable-web-security".to_string(),
            
            // Disable isolation features that can be detected (from PoC)
            "--disable-features=IsolateOrigins,site-per-process".to_string(),
            "--disable-site-isolation-trials".to_string(),
            "--allow-running-insecure-content".to_string(),
            
            // Window and display settings (from PoC)
            "--window-size=1920,1080".to_string(),
            "--start-maximized".to_string(),
            "--force-device-scale-factor=1".to_string(),
            
            // Security and sandboxing (from PoC)
            "--no-sandbox".to_string(),
            "--disable-setuid-sandbox".to_string(),
            "--disable-dev-shm-usage".to_string(),
            
            // Performance and GPU settings (from PoC)
            "--disable-accelerated-2d-canvas".to_string(),
            "--no-first-run".to_string(),
            "--no-zygote".to_string(),
            "--disable-gpu".to_string(),
            
            // Background process control (from PoC)
            "--disable-background-timer-throttling".to_string(),
            "--disable-backgrounding-occluded-windows".to_string(),
            "--disable-renderer-backgrounding".to_string(),
            
            // Feature disabling for stealth (from PoC)
            "--disable-features=TranslateUI".to_string(),
            "--disable-ipc-flooding-protection".to_string(),
            "--no-default-browser-check".to_string(),
            "--disable-hang-monitor".to_string(),
            "--disable-prompt-on-repost".to_string(),
            "--disable-sync".to_string(),
            "--disable-domain-reliability".to_string(),
            "--disable-client-side-phishing-detection".to_string(),
            "--disable-component-update".to_string(),
            "--disable-default-apps".to_string(),
            "--disable-extensions".to_string(),
            
            // Additional stealth features (from PoC)
            "--disable-features=ChromeWhatsNewUI".to_string(),
            "--disable-features=ImprovedCookieControls".to_string(),
            "--disable-features=LazyFrameLoading".to_string(),
            "--disable-features=GlobalMediaControls".to_string(),
            "--disable-features=DestroyProfileOnBrowserClose".to_string(),
            "--disable-features=MediaRouter".to_string(),
            "--disable-features=DialMediaRouteProvider".to_string(),
            "--disable-features=AcceptCHFrame".to_string(),
            "--disable-features=AutoExpandDetailsElement".to_string(),
            "--disable-features=CertificateTransparencyComponentUpdater".to_string(),
            "--disable-features=AvoidUnnecessaryBeforeUnloadCheckSync".to_string(),
            "--disable-features=Translate".to_string(),
            
            // Memory optimization (from PoC)
            "--memory-pressure-off".to_string(),
            "--max_old_space_size=4096".to_string(),
            
            // Enable useful features (from PoC)
            "--enable-features=NetworkService,NetworkServiceInProcess".to_string(),
            "--enable-automation".to_string(), // Counterintuitively helps with stability
            "--disable-blink-features".to_string(),
            "--disable-popup-blocking".to_string(),
            "--disable-notifications".to_string(),
        ]
    }

    /// Get trending tokens by scraping DexScreener website
    pub async fn get_trending_tokens_scraped(
        &mut self,
        chain: &str,
        timeframe: &str
    ) -> Result<Vec<DexScreenerTrendingToken>, DexScreenerError> {
        if !self.config.enabled {
            return Ok(vec![]);
        }
        
        let browser = self.ensure_browser().await?;
        let page = browser.new_page("about:blank")
            .await
            .map_err(|e| DexScreenerError::BrowserError(format!("Failed to create page: {}", e)))?;
        
        // Configure anti-detection BEFORE navigation (critical from PoC)
        if self.config.anti_detection_enabled {
            self.setup_anti_detection(&page).await?;
        }
        
        // Use chain-specific URL for optimized data retrieval (no client-side filtering needed)
        let timeframe_param = self.get_timeframe_param(timeframe);
        let chain_path = self.get_dexscreener_chain_path(chain);
        let url = format!("https://dexscreener.com/{}?rankBy={}&order=desc", chain_path, timeframe_param);
        
        debug!("🔍 Navigating to DexScreener trending: {} for {} chain", url, chain);
        
        // Add random delay before navigation (human-like behavior from PoC)
        let pre_nav_delay = {
            let mut rng = rand::thread_rng();
            rng.gen_range(500..1500)
        };
        tokio::time::sleep(Duration::from_millis(pre_nav_delay)).await;
        
        // Navigate with explicit 60-second timeout (matching working JS scraper)
        let nav_result = tokio::time::timeout(
            Duration::from_secs(60),
            page.goto(&url)
        ).await;

        match nav_result {
            Ok(Ok(_)) => {
                debug!("✅ Successfully navigated to: {}", url);
            },
            Ok(Err(e)) => {
                return Err(DexScreenerError::BrowserError(format!("Navigation failed: {}", e)));
            },
            Err(_) => {
                return Err(DexScreenerError::BrowserError(format!("Navigation timed out after 60 seconds: {}", url)));
            }
        }
        
        // Wait for table elements to load (matching JS scraper approach)
        let table_wait_result = tokio::time::timeout(
            Duration::from_secs(15),
            async {
                // Try multiple selectors as fallback (matching working JS scraper)
                for selector in [".ds-dex-table-row", ".ds-dex-table-top", "table", "[class*=\"table\"]"] {
                    if page.find_element(selector).await.is_ok() {
                        debug!("✅ Found table selector: {}", selector);
                        return Ok(());
                    }
                }
                Err("No table selectors found")
            }
        ).await;

        match table_wait_result {
            Ok(Ok(_)) => debug!("✅ Table elements loaded successfully"),
            Ok(Err(e)) => warn!("⚠️ Table selector not found, proceeding anyway: {}", e),
            Err(_) => warn!("⚠️ Table loading timed out after 15 seconds, proceeding anyway"),
        }
        
        // Additional wait for JavaScript to execute (from PoC)
        let load_delay = {
            let mut rng = rand::thread_rng();
            rng.gen_range(2000..4000)
        };
        debug!("Table found, waiting {} ms for JavaScript to fully execute...", load_delay);
        tokio::time::sleep(Duration::from_millis(load_delay)).await;
        
        // Simulate human behavior: random scrolling (from PoC)
        self.simulate_human_behavior(&page).await?;
        
        // Additional wait after human simulation
        let post_sim_delay = {
            let mut rng = rand::thread_rng();
            rng.gen_range(2000..4000)
        };
        tokio::time::sleep(Duration::from_millis(post_sim_delay)).await;
        
        // Extract tokens with retry logic (from PoC)
        // Try server data extraction first (primary method)
        let tokens = match self.extract_trending_tokens_from_server_data(&page, chain).await {
            Ok(server_tokens) if !server_tokens.is_empty() => {
                info!("✅ Successfully extracted {} tokens using server data for {}", server_tokens.len(), chain);
                server_tokens
            }
            Ok(_) => {
                warn!("⚠️ Server data extraction returned 0 tokens, falling back to DOM parsing for {}", chain);
                self.extract_trending_tokens_with_retry(&page, chain).await?
            }
            Err(e) => {
                warn!("⚠️ Server data extraction failed for {}: {}, falling back to DOM parsing", chain, e);
                self.extract_trending_tokens_with_retry(&page, chain).await?
            }
        };
        
        if self.config.rate_limit_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.config.rate_limit_delay_ms)).await;
        }
        
        info!("📊 Retrieved {} trending tokens from DexScreener for {} ({})", tokens.len(), chain, timeframe);
        Ok(tokens)
    }
    
    /// Get all trending tokens across all supported chains and timeframes
    pub async fn get_all_trending_tokens_scraped(&mut self) -> Result<Vec<DexScreenerTrendingToken>, DexScreenerError> {
        let chains = vec!["solana", "bsc", "ethereum", "base"];
        let timeframes = vec!["trendingScoreM5", "trendingScoreH1", "trendingScoreH6", "trendingScoreH24"];
        
        let mut all_tokens = Vec::new();
        let mut unique_addresses = std::collections::HashSet::new();
        
        for chain in chains {
            for timeframe in &timeframes {
                match self.get_trending_tokens_scraped(chain, timeframe).await {
                    Ok(tokens) => {
                        for token in tokens {
                            // Only add unique tokens based on address + chain
                            let key = format!("{}_{}", token.address, token.chain_id);
                            if unique_addresses.insert(key) {
                                all_tokens.push(token);
                            }
                        }
                    }
                    Err(e) => {
                        warn!("Failed to get trending tokens for {} {}: {}", chain, timeframe, e);
                        // Continue with other chains/timeframes
                    }
                }
                
                // Delay between requests
                tokio::time::sleep(Duration::from_millis(self.config.rate_limit_delay_ms)).await;
            }
        }
        
        // Sort by volume descending
        all_tokens.sort_by(|a, b| {
            b.volume_24h.unwrap_or(0.0).partial_cmp(&a.volume_24h.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });
        
        info!("🎯 Retrieved {} unique trending tokens across all chains and timeframes", all_tokens.len());
        Ok(all_tokens)
    }
    
    /// Setup comprehensive stealth JavaScript (mirrored from working PoC)
    async fn setup_anti_detection(&self, page: &Page) -> Result<(), DexScreenerError> {
        // Comprehensive stealth script from working PoC
        let stealth_js = r#"
            // Override webdriver detection
            Object.defineProperty(navigator, 'webdriver', {
                get: () => undefined
            });
            
            // Mock Chrome object
            if (!window.chrome) {
                window.chrome = {};
            }
            window.chrome.runtime = {};
            window.chrome.loadTimes = function() {};
            window.chrome.csi = function() {};
            window.chrome.app = {
                isInstalled: false
            };
            
            // Override automation-specific properties
            Object.defineProperty(navigator, 'plugins', {
                get: () => {
                    return {
                        length: 3,
                        0: { name: 'Chrome PDF Plugin', filename: 'internal-pdf-viewer' },
                        1: { name: 'Chrome PDF Viewer', filename: 'mhjfbmdgcfjbbpaeojofohoefgiehjai' },
                        2: { name: 'Native Client', filename: 'internal-nacl-plugin' }
                    };
                }
            });
            
            // Mock languages
            Object.defineProperty(navigator, 'languages', {
                get: () => ['en-US', 'en']
            });
            
            // Mock platform
            Object.defineProperty(navigator, 'platform', {
                get: () => 'Win32'
            });
            
            // Override device memory
            Object.defineProperty(navigator, 'deviceMemory', {
                get: () => 8
            });
            
            // Mock connection
            Object.defineProperty(navigator, 'connection', {
                get: () => ({
                    effectiveType: '4g',
                    rtt: 100,
                    downlink: 10.0,
                    saveData: false
                })
            });
            
            // Remove automation-related properties
            delete navigator.__proto__.webdriver;
            
            // Override toString methods to appear native
            window.navigator.permissions.query.toString = () => 'function query() { [native code] }';
            
            // Console log override
            const originalLog = console.log;
            console.log = function() {
                if (arguments[0] && arguments[0].includes && arguments[0].includes('HeadlessChrome')) {
                    return;
                }
                return originalLog.apply(console, arguments);
            };
            
            // Override other detection methods
            Object.defineProperty(navigator, 'hardwareConcurrency', {
                get: () => 4
            });
            
            Object.defineProperty(screen, 'colorDepth', {
                get: () => 24
            });
            
            Object.defineProperty(screen, 'pixelDepth', {
                get: () => 24
            });
        "#;
        
        // Execute stealth script
        page.evaluate(stealth_js).await
            .map_err(|e| DexScreenerError::BrowserError(format!("Failed to setup anti-detection: {}", e)))?;
        
        // Set random user agent (from PoC)
        let user_agents = vec![
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            "Mozilla/5.0 (Windows NT 10.0; Win64; x64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/119.0.0.0 Safari/537.36",
            "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
            "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/120.0.0.0 Safari/537.36",
        ];
        
        let user_agent = {
            let mut rng = rand::thread_rng();
            user_agents[rng.gen_range(0..user_agents.len())]
        };
        page.set_user_agent(user_agent).await
            .map_err(|e| DexScreenerError::BrowserError(format!("Failed to set user agent: {}", e)))?;
        
        debug!("Stealth mode activated with user agent: {}", user_agent);
        
        Ok(())
    }
    
    /// Extract trending tokens from server data (primary method)
    async fn extract_trending_tokens_from_server_data(&self, page: &Page, chain: &str) -> Result<Vec<DexScreenerTrendingToken>, DexScreenerError> {
        let script = r#"
            (() => {
                try {
                    // Check for server data availability (CORRECTED PATH)
                    if (!window.__SERVER_DATA?.route?.data?.pairs) {
                        console.log('❌ No server data available');
                        return { success: false, pairs: [], error: 'No server data' };
                    }
                    
                    const pairs = window.__SERVER_DATA.route.data.pairs;
                    console.log(`✅ Found ${pairs.length} pairs in server data`);
                    
                    const tokens = pairs.map((pair, index) => {
                        try {
                            // Extract actual token address from baseToken (not pair address)
                            const tokenAddress = pair.baseToken?.address || 'unknown';
                            const pairAddress = pair.pairAddress || 'unknown';
                            const symbol = pair.baseToken?.symbol || 'Unknown';
                            const name = pair.baseToken?.name || 'Unknown';
                            
                            // Calculate age in days from creation timestamp
                            let ageInDays = 0;
                            if (pair.pairCreatedAt) {
                                ageInDays = Math.floor((Date.now() - pair.pairCreatedAt) / (1000 * 60 * 60 * 24));
                            }
                            
                            // Calculate total transactions (buys + sells)
                            const transactions = (pair.txns?.h24?.buys || 0) + (pair.txns?.h24?.sells || 0);
                            
                            // Format volume as number
                            const volume = pair.volume?.h24 || 0;
                            
                            // Price change percentage
                            const priceChange = pair.priceChange?.h24 || 0;
                            
                            const token = {
                                address: tokenAddress, // ACTUAL TOKEN ADDRESS from baseToken.address
                                symbol: symbol,
                                name: name,
                                decimals: pair.baseToken?.decimals || null,
                                price: parseFloat(pair.priceUsd || '0'),
                                priceChange24h: priceChange,
                                volume24h: volume,
                                liquidity: pair.liquidity?.usd || null,
                                fdv: pair.fdv || null,
                                marketcap: pair.marketCap || null,
                                rank: index + 1,
                                logoUri: pair.baseToken?.logoURI || null,
                                txns24h: transactions,
                                chainId: chain, // embedded chain parameter
                                pairAddress: pairAddress,
                                makers24h: pair.makers?.h24 || 0,
                                ageInDays: ageInDays
                            };
                            
                            return token;
                            
                        } catch (error) {
                            console.error(`❌ Error processing pair ${index}:`, error.message);
                            return null;
                        }
                    }).filter(token => token !== null);
                    
                    console.log(`📋 Successfully extracted ${tokens.length} tokens from server data`);
                    return { success: true, pairs: tokens, error: null };
                    
                } catch (error) {
                    console.error('❌ Error extracting from server data:', error);
                    return { success: false, pairs: [], error: error.message };
                }
            })()
        "#;
        
        // Create script with chain parameter embedded
        let script_with_chain = format!("(function() {{ const chain = '{}'; return ({}); }})()", chain, script);
        
        let result = page.evaluate(script_with_chain.as_str()).await
            .map_err(|e| DexScreenerError::BrowserError(format!("Failed to extract server data: {}", e)))?;
        
        let extraction_result: serde_json::Value = result.value()
            .ok_or_else(|| DexScreenerError::BrowserError("No result from server data extraction".to_string()))?
            .clone();
        
        let success = extraction_result.get("success")
            .and_then(|s| s.as_bool())
            .unwrap_or(false);
            
        if !success {
            let error = extraction_result.get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown error");
            return Err(DexScreenerError::BrowserError(format!("Server data extraction failed: {}", error)));
        }
        
        let pairs_data = extraction_result.get("pairs")
            .ok_or_else(|| DexScreenerError::BrowserError("No pairs data in result".to_string()))?;
        
        let mut tokens = Vec::new();
        if let Some(pairs_array) = pairs_data.as_array() {
            for pair_data in pairs_array {
                if let Some(token) = self.convert_server_data_to_token(pair_data, chain) {
                    tokens.push(token);
                }
            }
        }
        
        debug!("✅ Extracted {} tokens from server data for {}", tokens.len(), chain);
        Ok(tokens)
    }
    
    /// Extract trending tokens from the page using DOM-based approach (fallback method)
    async fn extract_trending_tokens(&self, page: &Page, chain: &str) -> Result<Vec<DexScreenerTrendingToken>, DexScreenerError> {
        let script = r#"
            (() => {
                try {
                    // Check if we're on Cloudflare challenge page
                    if (document.title.includes('Just a moment') || document.body.innerText.includes('Cloudflare')) {
                        return null;
                    }
                    
                    const tokens = [];
                    const rows = document.querySelectorAll('.ds-dex-table-row');
                    console.log(`Found ${rows.length} DOM rows for scraping`);
                    
                    rows.forEach((row, index) => {
                        try {
                            // Multi-method token address extraction (adapted from working scraper)
                            let tokenAddress = 'unknown';
                            let pairAddress = 'unknown';
                            let extractionMethod = 'none';
                            
                            // Method 1: Extract from href pattern
                            const href = row.getAttribute('href') || '';
                            if (href) {
                                const pairMatch = href.match(/\/(solana|ethereum|bsc|base)\/([A-Za-z0-9]+)$/);
                                if (pairMatch) {
                                    pairAddress = pairMatch[2];
                                }
                            }
                            
                            // Method 2: Look for token-specific links within the row (/tokens/address)
                            const tokenLinks = row.querySelectorAll('a[href*="/tokens/"]');
                            if (tokenLinks.length > 0) {
                                const tokenUrl = tokenLinks[0].getAttribute('href');
                                if (tokenUrl) {
                                    const tokenMatch = tokenUrl.match(/\/tokens\/([A-Za-z0-9]+)/);
                                    if (tokenMatch) {
                                        tokenAddress = tokenMatch[1];
                                        extractionMethod = 'token_link';
                                    }
                                }
                            }
                            
                            // Method 3: Check data attributes for token addresses
                            if (tokenAddress === 'unknown') {
                                const dataAddress = row.getAttribute('data-token-address') || 
                                                  row.getAttribute('data-base-token-address') ||
                                                  row.getAttribute('data-token') ||
                                                  row.getAttribute('data-address');
                                if (dataAddress) {
                                    tokenAddress = dataAddress;
                                    extractionMethod = 'data_attribute';
                                }
                            }
                            
                            // Method 4: Search for token address patterns within the row
                            if (tokenAddress === 'unknown') {
                                const allLinks = row.querySelectorAll('a[href]');
                                for (const link of allLinks) {
                                    const linkHref = link.getAttribute('href') || '';
                                    let addressMatch = null;
                                    // Chain-specific pattern matching (using embedded chain variable)
                                    if (chain === 'solana') {
                                        addressMatch = linkHref.match(/([A-Za-z0-9]{32,44})/);
                                    } else {
                                        addressMatch = linkHref.match(/(0x[a-fA-F0-9]{40})/);
                                    }
                                    
                                    if (addressMatch && addressMatch[1] !== pairAddress) {
                                        tokenAddress = addressMatch[1];
                                        extractionMethod = 'href_pattern';
                                        break;
                                    }
                                }
                            }
                            
                            // Fallback: Use pair address if no specific token address found
                            if (tokenAddress === 'unknown') {
                                tokenAddress = pairAddress;
                                extractionMethod = 'pair_fallback';
                            }
                            
                            // Extract symbol with multiple selector fallbacks
                            let symbol = 'Unknown';
                            const symbolSelectors = [
                                '.ds-dex-table-row-base-token-symbol',
                                '.chakra-text.ds-dex-table-row-base-token-symbol',
                                '[class*="token-symbol"]',
                                '[class*="base-token-symbol"]'
                            ];
                            
                            for (const selector of symbolSelectors) {
                                const symbolEl = row.querySelector(selector);
                                if (symbolEl && symbolEl.textContent && symbolEl.textContent.trim()) {
                                    symbol = symbolEl.textContent.trim();
                                    break;
                                }
                            }
                            
                            // Extract name with multiple selector fallbacks
                            let name = 'Unknown';
                            const nameSelectors = [
                                '.ds-dex-table-row-base-token-name-text',
                                '[class*="token-name-text"]',
                                '[class*="base-token-name"]',
                                '[title]'
                            ];
                            
                            for (const selector of nameSelectors) {
                                const nameEl = row.querySelector(selector);
                                if (nameEl) {
                                    const nameText = nameEl.textContent && nameEl.textContent.trim() || 
                                                   nameEl.getAttribute('title') && nameEl.getAttribute('title').trim();
                                    if (nameText && nameText !== 'Unknown') {
                                        name = nameText;
                                        break;
                                    }
                                }
                            }
                            
                            // Extract price
                            const priceEl = row.querySelector('.ds-dex-table-row-col-price');
                            const priceText = priceEl ? priceEl.textContent.trim() : '$0';
                            
                            // Extract volume
                            const volumeEl = row.querySelector('.ds-dex-table-row-col-volume');
                            const volumeText = volumeEl ? volumeEl.textContent.trim() : '$0';
                            
                            // Extract transactions
                            const txnsEl = row.querySelector('.ds-dex-table-row-col-txns');
                            const txnsText = txnsEl ? txnsEl.textContent.trim().replace(/,/g, '') : '0';
                            const transactions = parseInt(txnsText) || 0;
                            
                            // Extract makers
                            const makersEl = row.querySelector('.ds-dex-table-row-col-makers');
                            const makersText = makersEl ? makersEl.textContent.trim().replace(/,/g, '') : '0';
                            const makers = parseInt(makersText) || 0;
                            
                            // Extract price change
                            let change = '0%';
                            const changeEls = row.querySelectorAll('[class*="price-change"] .ds-change-perc');
                            if (changeEls.length > 0) {
                                change = changeEls[changeEls.length - 1].textContent.trim() || '0%';
                            }
                            
                            // Extract pair age
                            const ageEl = row.querySelector('.ds-dex-table-row-col-pair-age span');
                            const age = ageEl ? ageEl.textContent.trim() : '0';
                            
                            tokens.push({
                                address: tokenAddress,
                                symbol: symbol,
                                name: name,
                                price: priceText,
                                volume: volumeText,
                                transactions: transactions,
                                makers: makers,
                                change: change,
                                age: age,
                                pairAddress: pairAddress,
                                extractionMethod: extractionMethod,
                                href: href
                            });
                            
                            // Log first few for debugging
                            if (index < 3) {
                                console.log(`Token ${index + 1}: ${symbol} (${name}) | Address: ${tokenAddress} | Method: ${extractionMethod}`);
                            }
                            
                        } catch (error) {
                            console.error(`Error parsing DOM row ${index}:`, {
                                error: error.message,
                                href: row.getAttribute('href'),
                                className: row.className
                            });
                        }
                    });
                    
                    // Calculate extraction method statistics
                    const methodStats = tokens.reduce((stats, token) => {
                        stats[token.extractionMethod] = (stats[token.extractionMethod] || 0) + 1;
                        return stats;
                    }, {});
                    
                    console.log(`Extraction Methods Used:`, methodStats);
                    console.log(`Total Tokens Extracted: ${tokens.length}`);
                    
                    return tokens;
                    
                } catch (e) {
                    console.log('Error:', e);
                    return null;
                }
            })()
        "#;
        
        // Create script with chain parameter embedded
        let script_with_chain = format!("(function() {{ const chain = '{}'; return ({}); }})()", chain, script);
        
        let result = page.evaluate(script_with_chain.as_str()).await
            .map_err(|e| DexScreenerError::BrowserError(format!("Failed to extract tokens: {}", e)))?;
        
        // Extract the JSON value from the evaluation result  
        let result_value: serde_json::Value = result.into_value()
            .map_err(|e| DexScreenerError::BrowserError(format!("Failed to get evaluation result: {}", e)))?;
            
        if result_value.is_null() {
            warn!("No DOM data found in page - likely Cloudflare challenge or empty page");
            return Ok(vec![]);
        }
        
        // Handle DOM-extracted tokens array directly
        let mut tokens = Vec::new();
        
        if let Some(tokens_array) = result_value.as_array() {
            // Convert DOM-extracted tokens to our DexScreenerTrendingToken structure
            for dom_token in tokens_array {
                if let Some(token) = self.convert_dom_token_to_trending_token(dom_token, chain) {
                    tokens.push(token);
                }
            }
        }
        
        debug!("Retrieved {} tokens for {} chain from DexScreener DOM extraction", tokens.len(), chain);
        Ok(tokens)
    }
    
    /// Convert DOM-extracted token data to our DexScreenerTrendingToken structure
    fn convert_dom_token_to_trending_token(&self, dom_token: &serde_json::Value, chain: &str) -> Option<DexScreenerTrendingToken> {
        // Parse price from string (e.g., "$0.001234" -> 0.001234)
        let price_text = dom_token.get("price")?.as_str().unwrap_or("$0");
        let price = price_text.trim_start_matches('$').replace(",", "").parse::<f64>().unwrap_or(0.0);
        
        // Parse volume from string (e.g., "$1.2M" -> convert to numeric)
        let volume_text = dom_token.get("volume")?.as_str().unwrap_or("$0");
        let volume_24h = self.parse_currency_value(volume_text);
        
        // Parse price change percentage
        let change_text = dom_token.get("change")?.as_str().unwrap_or("0%");
        let price_change_24h = change_text.trim_end_matches('%').parse::<f64>().ok();
        
        Some(DexScreenerTrendingToken {
            address: dom_token.get("address")?.as_str()?.to_string(),
            symbol: dom_token.get("symbol")?.as_str().unwrap_or("Unknown").to_string(),
            name: dom_token.get("name")?.as_str().unwrap_or("Unknown").to_string(),
            decimals: None, // Not available in DOM extraction
            price,
            price_change_24h,
            volume_24h: Some(volume_24h),
            volume_change_24h: None, // Not available
            liquidity: None, // Not directly available in DOM
            fdv: None, // Not directly available
            marketcap: None, // Not directly available
            rank: None, // Could be derived from position
            logo_uri: None, // Not available in DOM extraction
            txns_24h: dom_token.get("transactions").and_then(|t| t.as_u64()).map(|t| t as u32),
            last_trade_unix_time: None, // Not available
            chain_id: chain.to_string(),
            pair_address: dom_token.get("pairAddress")?.as_str().map(|s| s.to_string()),
        })
    }
    
    /// Parse currency values like "$1.2M", "$500K" to numeric values
    fn parse_currency_value(&self, value: &str) -> f64 {
        let clean_value = value.trim_start_matches('$').replace(",", "");
        
        if clean_value.ends_with('M') || clean_value.ends_with('m') {
            clean_value.trim_end_matches(['M', 'm']).parse::<f64>().unwrap_or(0.0) * 1_000_000.0
        } else if clean_value.ends_with('K') || clean_value.ends_with('k') {
            clean_value.trim_end_matches(['K', 'k']).parse::<f64>().unwrap_or(0.0) * 1_000.0
        } else if clean_value.ends_with('B') || clean_value.ends_with('b') {
            clean_value.trim_end_matches(['B', 'b']).parse::<f64>().unwrap_or(0.0) * 1_000_000_000.0
        } else {
            clean_value.parse::<f64>().unwrap_or(0.0)
        }
    }
    
    /// Convert server data to token structure (primary conversion method)
    fn convert_server_data_to_token(&self, token_data: &serde_json::Value, chain: &str) -> Option<DexScreenerTrendingToken> {
        Some(DexScreenerTrendingToken {
            address: token_data.get("address")?.as_str()?.to_string(),
            symbol: token_data.get("symbol")?.as_str()?.to_string(),
            name: token_data.get("name")?.as_str()?.to_string(),
            decimals: token_data.get("decimals").and_then(|d| d.as_u64()).map(|d| d as u8),
            price: token_data.get("price").and_then(|p| p.as_f64()).unwrap_or(0.0),
            price_change_24h: token_data.get("priceChange24h").and_then(|pc| pc.as_f64()),
            volume_24h: token_data.get("volume24h").and_then(|v| v.as_f64()),
            volume_change_24h: None, // Not available in server data
            liquidity: token_data.get("liquidity").and_then(|l| l.as_f64()),
            fdv: token_data.get("fdv").and_then(|f| f.as_f64()),
            marketcap: token_data.get("marketcap").and_then(|mc| mc.as_f64()),
            rank: token_data.get("rank").and_then(|r| r.as_u64()).map(|r| r as u32),
            logo_uri: token_data.get("logoUri").and_then(|l| l.as_str()).map(|s| s.to_string()),
            txns_24h: token_data.get("txns24h").and_then(|t| t.as_u64()).map(|t| t as u32),
            last_trade_unix_time: None, // Not readily available
            chain_id: chain.to_string(),
            pair_address: token_data.get("pairAddress").and_then(|pa| pa.as_str()).map(|s| s.to_string()),
        })
    }
    
    /// Convert DexScreener pair data to our token structure (legacy method - kept for compatibility)
    #[allow(dead_code)]
    fn convert_pair_to_token(&self, pair: &serde_json::Value, chain: &str) -> Option<DexScreenerTrendingToken> {
        let base_token = pair.get("baseToken")?;
        let price_usd = pair.get("priceUsd").and_then(|p| p.as_str()).and_then(|s| s.parse::<f64>().ok()).unwrap_or(0.0);
        let volume_24h = pair.get("volume").and_then(|v| v.get("h24")).and_then(|h| h.as_f64());
        let price_change_24h = pair.get("priceChange").and_then(|pc| pc.get("h24")).and_then(|h| h.as_f64());
        let liquidity_usd = pair.get("liquidity").and_then(|l| l.get("usd")).and_then(|u| u.as_f64());
        let fdv = pair.get("fdv").and_then(|f| f.as_f64());
        let market_cap = pair.get("marketCap").and_then(|mc| mc.as_f64());
        let txns_24h = pair.get("txns").and_then(|t| t.get("h24"))
            .and_then(|h| {
                let buys = h.get("buys").and_then(|b| b.as_u64()).unwrap_or(0);
                let sells = h.get("sells").and_then(|s| s.as_u64()).unwrap_or(0);
                Some((buys + sells) as u32)
            });
        
        Some(DexScreenerTrendingToken {
            address: base_token.get("address")?.as_str()?.to_string(),
            symbol: base_token.get("symbol")?.as_str()?.to_string(),
            name: base_token.get("name")?.as_str()?.to_string(),
            decimals: base_token.get("decimals").and_then(|d| d.as_u64()).map(|d| d as u8),
            price: price_usd,
            price_change_24h,
            volume_24h,
            volume_change_24h: None, // Not available in DexScreener
            liquidity: liquidity_usd,
            fdv,
            marketcap: market_cap,
            rank: None, // Could be derived from position in array
            logo_uri: base_token.get("logoURI").and_then(|l| l.as_str()).map(|s| s.to_string()),
            txns_24h,
            last_trade_unix_time: None, // Not readily available
            chain_id: chain.to_string(),
            pair_address: pair.get("pairAddress").and_then(|pa| pa.as_str()).map(|s| s.to_string()),
        })
    }
    
    /// Get timeframe parameter for DexScreener
    fn get_timeframe_param(&self, timeframe: &str) -> &str {
        match timeframe {
            "5m" | "trendingScoreM5" => "trendingScoreM5",
            "1h" | "trendingScoreH1" => "trendingScoreH1",
            "6h" | "trendingScoreH6" => "trendingScoreH6",
            "24h" | "trendingScoreH24" => "trendingScoreH24",
            _ => "trendingScoreH24", // default to 24h
        }
    }
    
    /// Get DexScreener chain path for URL generation
    fn get_dexscreener_chain_path(&self, chain: &str) -> &str {
        match chain.to_lowercase().as_str() {
            "ethereum" | "eth" => "ethereum",
            "bsc" | "binance" => "bsc", 
            "base" => "base",
            "solana" => "solana",
            _ => "ethereum" // fallback to ethereum for unknown chains
        }
    }
    
    /// Simulate human behavior (mirrored from working PoC)
    async fn simulate_human_behavior(&self, page: &Page) -> Result<(), DexScreenerError> {
        // Random scrolling behavior
        let scroll_count = {
            let mut rng = rand::thread_rng();
            rng.gen_range(2..5)
        };
        
        for _ in 0..scroll_count {
            let (scroll_y, scroll_delay) = {
                let mut rng = rand::thread_rng();
                (rng.gen_range(200..800), rng.gen_range(300..800))
            };
            
            let scroll_script = format!("window.scrollTo(0, {})", scroll_y);
            
            page.evaluate(scroll_script.as_str()).await
                .map_err(|e| DexScreenerError::BrowserError(format!("Failed to scroll: {}", e)))?;
                
            // Random delay between scrolls
            tokio::time::sleep(Duration::from_millis(scroll_delay)).await;
        }
        
        // Scroll back to top
        page.evaluate("window.scrollTo(0, 0)").await
            .map_err(|e| DexScreenerError::BrowserError(format!("Failed to scroll to top: {}", e)))?;
            
        debug!("Human behavior simulation completed");
        Ok(())
    }
    
    /// Extract trending tokens with retry logic (mirrored from working PoC)
    async fn extract_trending_tokens_with_retry(&self, page: &Page, chain: &str) -> Result<Vec<DexScreenerTrendingToken>, DexScreenerError> {
        let mut retries = 0;
        let max_retries = 10;
        
        loop {
            retries += 1;
            debug!("Checking for server data... (attempt {}/{})", retries, max_retries);
            
            // First check if page loaded properly
            let title_result = page.evaluate("() => document.title").await;
            debug!("Page title: {:?}", title_result);
            
            // Extract tokens using our comprehensive script
            let tokens = self.extract_trending_tokens(page, chain).await?;
            
            if !tokens.is_empty() {
                debug!("Found {} trending tokens for {} chain", tokens.len(), chain);
                return Ok(tokens);
            }
            
            if retries >= max_retries {
                warn!("Failed to load trending data after {} retries", max_retries);
                return Ok(vec![]); // Return empty instead of error for graceful degradation
            }
            
            // Random delay between retries (human-like)
            let retry_delay = {
                let mut rng = rand::thread_rng();
                rng.gen_range(1500..3000)
            };
            debug!("Waiting for data to load... (attempt {}/{}) - {} ms", retries, max_retries, retry_delay);
            tokio::time::sleep(Duration::from_millis(retry_delay)).await;
        }
    }
}

