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
// Import TopTrader for scraping integration
use crate::birdeye_client::TopTrader;

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
    pub scraperapi_key: String,
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
            scraperapi_key: "".to_string(),
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

            let (browser, mut handler) = Browser::launch(browser_config.build().map_err(|e| {
                DexScreenerError::BrowserError(format!("Browser config error: {}", e))
            })?)
            .await
            .map_err(|e| {
                DexScreenerError::BrowserError(format!("Failed to launch browser: {}", e))
            })?;

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
    pub async fn get_latest_boosted_tokens(
        &self,
    ) -> Result<Vec<DexScreenerBoostedToken>, DexScreenerError> {
        if !self.config.enabled {
            return Ok(vec![]);
        }

        let url = format!("{}/token-boosts/latest/v1", self.config.api_base_url);
        debug!("🔍 Fetching latest boosted tokens from: {}", url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(DexScreenerError::ApiError { status, message });
        }

        // DexScreener API returns an array of boosted tokens
        let boosted_tokens: Vec<DexScreenerBoostedToken> = response.json().await?;

        // Return all tokens without chain filtering to support multichain
        info!(
            "📊 Retrieved {} latest boosted tokens from DexScreener (all chains)",
            boosted_tokens.len()
        );

        if self.config.rate_limit_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.config.rate_limit_delay_ms)).await;
        }

        Ok(boosted_tokens)
    }

    /// Get the top boosted tokens (most active boosts)
    pub async fn get_top_boosted_tokens(
        &self,
    ) -> Result<Vec<DexScreenerBoostedToken>, DexScreenerError> {
        if !self.config.enabled {
            return Ok(vec![]);
        }

        let url = format!("{}/token-boosts/top/v1", self.config.api_base_url);
        debug!("🔍 Fetching top boosted tokens from: {}", url);

        let response = self.client.get(&url).send().await?;

        if !response.status().is_success() {
            let status = response.status().as_u16();
            let message = response
                .text()
                .await
                .unwrap_or_else(|_| "Unknown error".to_string());
            return Err(DexScreenerError::ApiError { status, message });
        }

        // DexScreener API returns an array of boosted tokens
        let boosted_tokens: Vec<DexScreenerBoostedToken> = response.json().await?;

        // Return all tokens without chain filtering to support multichain
        info!(
            "📊 Retrieved {} top boosted tokens from DexScreener (all chains)",
            boosted_tokens.len()
        );

        if self.config.rate_limit_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.config.rate_limit_delay_ms)).await;
        }

        Ok(boosted_tokens)
    }

    /// Get both latest and top boosted tokens in a single call
    pub async fn get_all_boosted_tokens(
        &self,
    ) -> Result<(Vec<DexScreenerBoostedToken>, Vec<DexScreenerBoostedToken>), DexScreenerError>
    {
        if !self.config.enabled {
            return Ok((vec![], vec![]));
        }

        debug!("🔍 Fetching all boosted tokens from DexScreener");

        let latest_tokens = self.get_latest_boosted_tokens().await?;
        let top_tokens = self.get_top_boosted_tokens().await?;

        debug!(
            "✅ Retrieved {} latest + {} top boosted tokens",
            latest_tokens.len(),
            top_tokens.len()
        );

        Ok((latest_tokens, top_tokens))
    }

    /// Extract unique token addresses from boosted tokens
    pub fn extract_token_addresses(
        &self,
        boosted_tokens: &[DexScreenerBoostedToken],
    ) -> Vec<String> {
        let mut addresses: Vec<String> = boosted_tokens
            .iter()
            .map(|token| token.token_address.clone())
            .collect();

        // Remove duplicates and sort
        addresses.sort();
        addresses.dedup();

        debug!(
            "📋 Extracted {} unique token addresses from boosted tokens",
            addresses.len()
        );
        addresses
    }

    /// Get only the token addresses from boosted tokens (convenience method)
    pub fn get_token_addresses(&self, boosted_tokens: &[DexScreenerBoostedToken]) -> Vec<String> {
        boosted_tokens
            .iter()
            .map(|token| token.token_address.clone())
            .collect()
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

    /// Fetch HTML via ScraperAPI HTTP proxy (no browser needed)
    /// Matching ScraperAPI documentation examples:
    /// - JavaScript: proxy: { host: 'proxy-server.scraperapi.com', port: 8001, auth: { username: 'scraperapi', password: 'KEY' } }
    /// - curl: curl -x "scraperapi:KEY@proxy-server.scraperapi.com:8001" -k "https://dexscreener.com/..."
    async fn fetch_html_via_scraperapi(
        &self,
        url: &str,
    ) -> Result<String, DexScreenerError> {
        // ScraperAPI proxy configuration (matching their documentation)
        // Host: proxy-server.scraperapi.com
        // Port: 8001
        // Auth: username=scraperapi, password=API_KEY
        let proxy_url = format!(
            "http://scraperapi:{}@proxy-server.scraperapi.com:8001",
            self.config.scraperapi_key
        );

        info!("🔒 Configuring ScraperAPI proxy: proxy-server.scraperapi.com:8001");
        info!("🌐 Target URL: {}", url);

        // Create proxy for all protocols (HTTP and HTTPS)
        let proxy = reqwest::Proxy::all(&proxy_url).map_err(|e| {
            error!("❌ Failed to create proxy: {}", e);
            DexScreenerError::BrowserError(format!("Failed to create proxy: {}", e))
        })?;

        // Build HTTP client with proxy and SSL verification disabled (like curl -k)
        // ScraperAPI handles captchas/retries internally, so use very long timeout
        let client = reqwest::Client::builder()
            .proxy(proxy)
            .danger_accept_invalid_certs(true) // Equivalent to curl -k
            .timeout(std::time::Duration::from_secs(300)) // 5 minutes for ScraperAPI processing
            .connect_timeout(std::time::Duration::from_secs(30)) // 30s to establish connection
            .pool_idle_timeout(std::time::Duration::from_secs(90))
            .pool_max_idle_per_host(10)
            .tcp_keepalive(std::time::Duration::from_secs(60))
            .build()
            .map_err(|e| {
                error!("❌ Failed to build HTTP client: {}", e);
                DexScreenerError::BrowserError(format!("Failed to build client: {}", e))
            })?;

        info!("📡 Sending GET request via ScraperAPI...");

        let response = client.get(url).send().await.map_err(|e| {
            error!("❌ ScraperAPI request failed: {}", e);
            DexScreenerError::HttpError(e)
        })?;

        let status = response.status();
        info!("📬 ScraperAPI response: HTTP {}", status);

        if !status.is_success() {
            error!("❌ ScraperAPI error: HTTP {}", status);
            return Err(DexScreenerError::BrowserError(format!(
                "HTTP {} for URL: {}",
                status,
                url
            )));
        }

        let html = response.text().await.map_err(|e| {
            error!("❌ Failed to read response: {}", e);
            DexScreenerError::HttpError(e)
        })?;

        info!("✅ ScraperAPI success: {} bytes", html.len());
        debug!("HTML preview (first 500 chars): {}", &html[..html.len().min(500)]);

        Ok(html)
    }

    /// Parse window.__SERVER_DATA from HTML to extract trending tokens
    fn parse_server_data_from_html(
        &self,
        html: &str,
        chain: &str,
    ) -> Result<Vec<DexScreenerTrendingToken>, DexScreenerError> {
        // Extract window.__SERVER_DATA JSON from HTML
        // Pattern: window.__SERVER_DATA = {large JSON object};
        // We need to match the entire JSON object, which can be multi-line and contain semicolons

        // Find the start of the assignment
        let start_marker = "window.__SERVER_DATA = ";
        let start_pos = html.find(start_marker).ok_or_else(|| {
            warn!("❌ No window.__SERVER_DATA found in HTML");
            warn!("HTML preview (first 1000 chars): {}", &html[..html.len().min(1000)]);
            DexScreenerError::BrowserError("No window.__SERVER_DATA in HTML".to_string())
        })?;

        let json_start = start_pos + start_marker.len();

        // Find the matching closing brace by counting braces
        let mut brace_count = 0;
        let mut json_end = json_start;
        let chars: Vec<char> = html[json_start..].chars().collect();

        for (i, ch) in chars.iter().enumerate() {
            match ch {
                '{' => brace_count += 1,
                '}' => {
                    brace_count -= 1;
                    if brace_count == 0 {
                        json_end = json_start + i + 1;
                        break;
                    }
                }
                _ => {}
            }
        }

        if brace_count != 0 {
            error!("❌ Unmatched braces in window.__SERVER_DATA JSON");
            return Err(DexScreenerError::BrowserError(
                "Malformed JSON in window.__SERVER_DATA".to_string()
            ));
        }

        let json_str = &html[json_start..json_end];

        debug!("📋 Extracted window.__SERVER_DATA JSON ({} bytes)", json_str.len());

        // Decode HTML entities (e.g., &quot;, &amp;, &#39;, etc.)
        let decoded_json = json_str
            .replace("&quot;", "\"")
            .replace("&amp;", "&")
            .replace("&#39;", "'")
            .replace("&lt;", "<")
            .replace("&gt;", ">")
            .replace("&#x2F;", "/");

        // Convert JavaScript syntax to JSON-compatible syntax
        // window.__SERVER_DATA contains JavaScript code, not pure JSON
        let json_compatible = decoded_json
            .replace(":undefined", ":null")      // undefined → null
            .replace(",undefined", ",null")      // undefined in arrays
            .replace("[undefined", "[null")      // undefined at array start
            .replace(":NaN", ":null")            // NaN → null
            .replace(",NaN", ",null")            // NaN in arrays
            .replace(":Infinity", ":null")       // Infinity → null
            .replace(":-Infinity", ":null");     // -Infinity → null

        debug!("📋 Converted to JSON-compatible ({} bytes)", json_compatible.len());

        let data: serde_json::Value = serde_json::from_str(&json_compatible).map_err(|e| {
            error!("❌ Failed to parse window.__SERVER_DATA JSON: {}", e);
            error!("JSON preview (first 500 chars): {}", &json_compatible[..json_compatible.len().min(500)]);
            DexScreenerError::BrowserError(format!("Invalid JSON in window.__SERVER_DATA: {}", e))
        })?;

        // Extract pairs array
        let pairs = data["route"]["data"]["pairs"]
            .as_array()
            .ok_or_else(|| {
                warn!("❌ No pairs array found in window.__SERVER_DATA");
                DexScreenerError::BrowserError("No pairs array in server data".to_string())
            })?;

        info!("📊 Found {} pairs in window.__SERVER_DATA", pairs.len());

        // Map JSON pairs to DexScreenerTrendingToken structs
        let tokens: Vec<DexScreenerTrendingToken> = pairs
            .iter()
            .enumerate()
            .filter_map(|(idx, pair)| {
                let token = DexScreenerTrendingToken {
                    address: pair["baseToken"]["address"].as_str()?.to_string(),
                    symbol: pair["baseToken"]["symbol"].as_str()?.to_string(),
                    name: pair["baseToken"]["name"].as_str()?.to_string(),
                    decimals: pair["baseToken"]["decimals"].as_i64().map(|d| d as u8),
                    price: pair["priceUsd"]
                        .as_str()
                        .and_then(|s| s.parse::<f64>().ok())
                        .unwrap_or(0.0),
                    price_change_24h: pair["priceChange"]["h24"].as_f64(),
                    volume_24h: pair["volume"]["h24"].as_f64(),
                    volume_change_24h: None, // Not provided in window.__SERVER_DATA
                    liquidity: pair["liquidity"]["usd"].as_f64(),
                    fdv: pair["fdv"].as_f64(),
                    marketcap: pair["marketCap"].as_f64(),
                    rank: Some((idx + 1) as u32),
                    logo_uri: pair["baseToken"]["logoURI"].as_str().map(|s| s.to_string()),
                    txns_24h: Some(
                        (pair["txns"]["h24"]["buys"].as_i64().unwrap_or(0)
                            + pair["txns"]["h24"]["sells"].as_i64().unwrap_or(0))
                            as u32,
                    ),
                    last_trade_unix_time: pair["pairCreatedAt"].as_i64(),
                    chain_id: chain.to_string(),
                    pair_address: pair["pairAddress"].as_str().map(|s| s.to_string()),
                };
                Some(token)
            })
            .collect();

        info!("✅ Successfully parsed {} tokens from HTML", tokens.len());

        Ok(tokens)
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
        timeframe: &str,
    ) -> Result<Vec<DexScreenerTrendingToken>, DexScreenerError> {
        if !self.config.enabled {
            return Ok(vec![]);
        }

        // Use chain-specific URL for optimized data retrieval (no client-side filtering needed)
        let timeframe_param = self.get_timeframe_param(timeframe);
        let chain_path = self.get_dexscreener_chain_path(chain);
        let url = format!(
            "https://dexscreener.com/{}?rankBy={}&order=desc",
            chain_path, timeframe_param
        );

        // Try ScraperAPI HTTP Proxy first (fast path)
        info!(
            "🚀 Attempting ScraperAPI Proxy Port method for trending tokens ({} {})",
            chain, timeframe
        );

        match self.fetch_html_via_scraperapi(&url).await {
            Ok(html) => {
                match self.parse_server_data_from_html(&html, chain) {
                    Ok(tokens) if !tokens.is_empty() => {
                        info!(
                            "✅ ScraperAPI Proxy succeeded: {} tokens for {} ({})",
                            tokens.len(), chain, timeframe
                        );

                        // Apply rate limiting
                        if self.config.rate_limit_delay_ms > 0 {
                            tokio::time::sleep(Duration::from_millis(self.config.rate_limit_delay_ms)).await;
                        }

                        return Ok(tokens);
                    }
                    Ok(_) => {
                        warn!("⚠️ ScraperAPI returned 0 tokens, falling back to browser");
                    }
                    Err(e) => {
                        warn!("⚠️ HTML parsing failed: {}, falling back to browser", e);
                    }
                }
            }
            Err(e) => {
                warn!("⚠️ ScraperAPI Proxy failed: {}, falling back to browser", e);
            }
        }

        // Fallback to browser method (without proxy - direct connection)
        info!("🌐 Using browser method for trending tokens ({} {})", chain, timeframe);

        let browser = self.ensure_browser().await?;
        let page = browser
            .new_page("about:blank")
            .await
            .map_err(|e| DexScreenerError::BrowserError(format!("Failed to create page: {}", e)))?;

        // Configure anti-detection BEFORE navigation (critical from PoC)
        if self.config.anti_detection_enabled {
            self.setup_anti_detection(&page).await?;
        }

        debug!(
            "🔍 Navigating to DexScreener trending: {} for {} chain",
            url, chain
        );

        // Add random delay before navigation (human-like behavior from PoC)
        let pre_nav_delay = {
            let mut rng = rand::thread_rng();
            rng.gen_range(500..1500)
        };
        tokio::time::sleep(Duration::from_millis(pre_nav_delay)).await;

        // Retry navigation up to 3 times if Cloudflare challenge fails to resolve
        let mut cloudflare_retry_count = 0;
        const MAX_CLOUDFLARE_RETRIES: u32 = 3;

        loop {
            // Navigate with explicit 60-second timeout (matching working JS scraper)
            let nav_result =
                tokio::time::timeout(Duration::from_secs(60), page.goto(&url)).await;

            match nav_result {
                Ok(Ok(_)) => {
                    debug!("✅ Successfully navigated to: {}", url);

                    // Add detailed page debugging
                    if let Ok(title) = page.get_title().await {
                        let page_title = title.unwrap_or("No title".to_string());
                        info!("📄 Page title: {}", page_title);
                    }

                    // Wait for Cloudflare challenge to resolve if detected
                    match self.wait_for_cloudflare_challenge(&page, &url).await {
                        Ok(_) => {
                            // Cloudflare resolved or not present, proceed

                            // Log page HTML (first 1000 chars for debugging)
                            if let Ok(html) = page.content().await {
                                let html_preview = if html.len() > 1000 {
                                    format!("{}...[TRUNCATED, total length: {}]", &html[..1000], html.len())
                                } else {
                                    html.clone()
                                };
                                debug!("🔍 Page HTML preview: {}", html_preview);

                                // Check for key indicators
                                if html.contains("dexscreener") {
                                    debug!("✅ Page contains 'dexscreener' - likely correct page");
                                } else {
                                    warn!("⚠️ Page does not contain 'dexscreener' - possible redirect or error page");
                                }

                                if html.contains("table") {
                                    debug!("✅ Page contains table elements");
                                } else {
                                    warn!("⚠️ Page does not contain any table elements");
                                }

                                if html.contains("trending") {
                                    debug!("✅ Page contains 'trending' - likely on correct page");
                                } else {
                                    warn!("⚠️ Page does not contain 'trending' - may not be on trending page");
                                }
                            }

                            break;
                        }
                        Err(e) => {
                            cloudflare_retry_count += 1;
                            if cloudflare_retry_count >= MAX_CLOUDFLARE_RETRIES {
                                error!(
                                    "❌ Cloudflare challenge failed to resolve after {} retries",
                                    MAX_CLOUDFLARE_RETRIES
                                );
                                return Err(e);
                            }

                            warn!(
                                "⚠️ Cloudflare challenge not resolved, retrying navigation (attempt {}/{})...",
                                cloudflare_retry_count + 1,
                                MAX_CLOUDFLARE_RETRIES
                            );

                            // Wait a bit before retry
                            tokio::time::sleep(Duration::from_secs(3)).await;
                            continue;
                        }
                    }
                }
                Ok(Err(e)) => {
                    return Err(DexScreenerError::BrowserError(format!(
                        "Navigation failed: {}",
                        e
                    )));
                }
                Err(_) => {
                    return Err(DexScreenerError::BrowserError(format!(
                        "Navigation timed out after 60 seconds: {}",
                        url
                    )));
                }
            }
        }

        // Wait for table elements to load (matching JS scraper approach)
        let table_wait_result = tokio::time::timeout(Duration::from_secs(15), async {
            // Try multiple selectors as fallback (matching working JS scraper)
            let selectors = [
                ".ds-dex-table-row",
                ".ds-dex-table-top",
                "table",
                "[class*=\"table\"]",
            ];

            info!("🔍 Searching for table selectors on page...");
            for selector in selectors {
                debug!("🔍 Trying selector: {}", selector);
                match page.find_element(selector).await {
                    Ok(element) => {
                        info!("✅ Found table selector: {}", selector);

                        // Log some details about the found element
                        if let Ok(Some(class_name)) = element.attribute("class").await {
                            debug!("📝 Element classes: {}", class_name);
                        }

                        return Ok(());
                    }
                    Err(e) => {
                        debug!("❌ Selector '{}' not found: {}", selector, e);
                    }
                }
            }

            // Log all available elements for debugging
            if let Ok(all_elements) = page.find_elements("*").await {
                let element_count = all_elements.len();
                info!("📊 Total elements found on page: {}", element_count);

                // Log first few element IDs for debugging
                for (i, element) in all_elements.iter().enumerate().take(10) {
                    if let Ok(Some(id)) = element.attribute("id").await {
                        if !id.is_empty() {
                            debug!("📝 Element {} id: {}", i, id);
                        }
                    }
                }
            }

            Err("No table selectors found")
        })
        .await;

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
        debug!(
            "Table found, waiting {} ms for JavaScript to fully execute...",
            load_delay
        );
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
        let tokens = match self
            .extract_trending_tokens_from_server_data(&page, chain)
            .await
        {
            Ok(server_tokens) if !server_tokens.is_empty() => {
                info!(
                    "✅ Successfully extracted {} tokens using server data for {}",
                    server_tokens.len(),
                    chain
                );
                server_tokens
            }
            Ok(_) => {
                warn!("⚠️ Server data extraction returned 0 tokens, falling back to DOM parsing for {}", chain);
                self.extract_trending_tokens_with_retry(&page, chain)
                    .await?
            }
            Err(e) => {
                warn!(
                    "⚠️ Server data extraction failed for {}: {}, falling back to DOM parsing",
                    chain, e
                );
                self.extract_trending_tokens_with_retry(&page, chain)
                    .await?
            }
        };

        if self.config.rate_limit_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(self.config.rate_limit_delay_ms)).await;
        }

        info!(
            "📊 Retrieved {} trending tokens from DexScreener for {} ({})",
            tokens.len(),
            chain,
            timeframe
        );
        Ok(tokens)
    }

    /// Get all trending tokens across all supported chains and timeframes
    pub async fn get_all_trending_tokens_scraped(
        &mut self,
    ) -> Result<Vec<DexScreenerTrendingToken>, DexScreenerError> {
        let chains = vec!["solana", "bsc", "ethereum", "base"];
        let timeframes = vec![
            "trendingScoreM5",
            "trendingScoreH1",
            "trendingScoreH6",
            "trendingScoreH24",
        ];

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
                        warn!(
                            "Failed to get trending tokens for {} {}: {}",
                            chain, timeframe, e
                        );
                        // Continue with other chains/timeframes
                    }
                }

                // Delay between requests
                tokio::time::sleep(Duration::from_millis(self.config.rate_limit_delay_ms)).await;
            }
        }

        // Sort by volume descending
        all_tokens.sort_by(|a, b| {
            b.volume_24h
                .unwrap_or(0.0)
                .partial_cmp(&a.volume_24h.unwrap_or(0.0))
                .unwrap_or(std::cmp::Ordering::Equal)
        });

        info!(
            "🎯 Retrieved {} unique trending tokens across all chains and timeframes",
            all_tokens.len()
        );
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
        page.evaluate(stealth_js).await.map_err(|e| {
            DexScreenerError::BrowserError(format!("Failed to setup anti-detection: {}", e))
        })?;

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
        page.set_user_agent(user_agent).await.map_err(|e| {
            DexScreenerError::BrowserError(format!("Failed to set user agent: {}", e))
        })?;

        debug!("Stealth mode activated with user agent: {}", user_agent);

        Ok(())
    }

    /// Extract trending tokens from server data (primary method)
    async fn extract_trending_tokens_from_server_data(
        &self,
        page: &Page,
        chain: &str,
    ) -> Result<Vec<DexScreenerTrendingToken>, DexScreenerError> {
        info!("🔍 Attempting server data extraction for chain: {}", chain);
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
        let script_with_chain = format!(
            "(function() {{ const chain = '{}'; return ({}); }})()",
            chain, script
        );

        let result = page
            .evaluate(script_with_chain.as_str())
            .await
            .map_err(|e| {
                DexScreenerError::BrowserError(format!("Failed to extract server data: {}", e))
            })?;

        let extraction_result: serde_json::Value = result
            .value()
            .ok_or_else(|| {
                warn!("❌ No result value from server data extraction script");
                DexScreenerError::BrowserError("No result from server data extraction".to_string())
            })?
            .clone();

        // Log the raw extraction result for debugging
        debug!("📋 Server data extraction result: {}", serde_json::to_string_pretty(&extraction_result).unwrap_or_else(|_| "Failed to serialize".to_string()));

        let success = extraction_result
            .get("success")
            .and_then(|s| s.as_bool())
            .unwrap_or(false);

        debug!("📊 Server data extraction success: {}", success);

        if !success {
            let error = extraction_result
                .get("error")
                .and_then(|e| e.as_str())
                .unwrap_or("Unknown error");

            warn!("❌ Server data extraction failed with error: {}", error);

            // Log full HTML when extraction fails (helps debug Cloudflare issues)
            if let Ok(html) = page.content().await {
                let html_preview = &html[..html.len().min(3000)];
                error!(
                    "🔍 Page HTML when server data extraction failed:\n{}",
                    html_preview
                );
            }

            // Additional debugging - log if window.__SERVER_DATA exists
            let check_server_data_script = r#"
                (() => {
                    const hasWindow = typeof window !== 'undefined';
                    const hasServerData = hasWindow && typeof window.__SERVER_DATA !== 'undefined';
                    const hasRoute = hasServerData && window.__SERVER_DATA.route;
                    const hasData = hasRoute && window.__SERVER_DATA.route.data;
                    const hasPairs = hasData && window.__SERVER_DATA.route.data.pairs;

                    return {
                        hasWindow,
                        hasServerData,
                        hasRoute,
                        hasData,
                        hasPairs,
                        pairCount: hasPairs ? window.__SERVER_DATA.route.data.pairs.length : 0
                    };
                })()
            "#;

            if let Ok(debug_result) = page.evaluate(check_server_data_script).await {
                if let Some(debug_value) = debug_result.value() {
                    debug!("🔍 Server data availability check: {}", serde_json::to_string_pretty(debug_value).unwrap_or_else(|_| "Failed to serialize".to_string()));
                }
            }

            return Err(DexScreenerError::BrowserError(format!(
                "Server data extraction failed: {}",
                error
            )));
        }

        let pairs_data = extraction_result
            .get("pairs")
            .ok_or_else(|| DexScreenerError::BrowserError("No pairs data in result".to_string()))?;

        let mut tokens = Vec::new();
        if let Some(pairs_array) = pairs_data.as_array() {
            for pair_data in pairs_array {
                if let Some(token) = self.convert_server_data_to_token(pair_data, chain) {
                    tokens.push(token);
                }
            }
        }

        debug!(
            "✅ Extracted {} tokens from server data for {}",
            tokens.len(),
            chain
        );
        Ok(tokens)
    }

    /// Extract trending tokens from the page using DOM-based approach (fallback method)
    async fn extract_trending_tokens(
        &self,
        page: &Page,
        chain: &str,
    ) -> Result<Vec<DexScreenerTrendingToken>, DexScreenerError> {
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
        let script_with_chain = format!(
            "(function() {{ const chain = '{}'; return ({}); }})()",
            chain, script
        );

        let result = page
            .evaluate(script_with_chain.as_str())
            .await
            .map_err(|e| {
                DexScreenerError::BrowserError(format!("Failed to extract tokens: {}", e))
            })?;

        // Extract the JSON value from the evaluation result
        let result_value: serde_json::Value = result.into_value().map_err(|e| {
            DexScreenerError::BrowserError(format!("Failed to get evaluation result: {}", e))
        })?;

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

        debug!(
            "Retrieved {} tokens for {} chain from DexScreener DOM extraction",
            tokens.len(),
            chain
        );
        Ok(tokens)
    }

    /// Convert DOM-extracted token data to our DexScreenerTrendingToken structure
    fn convert_dom_token_to_trending_token(
        &self,
        dom_token: &serde_json::Value,
        chain: &str,
    ) -> Option<DexScreenerTrendingToken> {
        // Parse price from string (e.g., "$0.001234" -> 0.001234)
        let price_text = dom_token.get("price")?.as_str().unwrap_or("$0");
        let price = price_text
            .trim_start_matches('$')
            .replace(",", "")
            .parse::<f64>()
            .unwrap_or(0.0);

        // Parse volume from string (e.g., "$1.2M" -> convert to numeric)
        let volume_text = dom_token.get("volume")?.as_str().unwrap_or("$0");
        let volume_24h = self.parse_currency_value(volume_text);

        // Parse price change percentage
        let change_text = dom_token.get("change")?.as_str().unwrap_or("0%");
        let price_change_24h = change_text.trim_end_matches('%').parse::<f64>().ok();

        Some(DexScreenerTrendingToken {
            address: dom_token.get("address")?.as_str()?.to_string(),
            symbol: dom_token
                .get("symbol")?
                .as_str()
                .unwrap_or("Unknown")
                .to_string(),
            name: dom_token
                .get("name")?
                .as_str()
                .unwrap_or("Unknown")
                .to_string(),
            decimals: None, // Not available in DOM extraction
            price,
            price_change_24h,
            volume_24h: Some(volume_24h),
            volume_change_24h: None, // Not available
            liquidity: None,         // Not directly available in DOM
            fdv: None,               // Not directly available
            marketcap: None,         // Not directly available
            rank: None,              // Could be derived from position
            logo_uri: None,          // Not available in DOM extraction
            txns_24h: dom_token
                .get("transactions")
                .and_then(|t| t.as_u64())
                .map(|t| t as u32),
            last_trade_unix_time: None, // Not available
            chain_id: chain.to_string(),
            pair_address: dom_token
                .get("pairAddress")?
                .as_str()
                .map(|s| s.to_string()),
        })
    }

    /// Parse currency values like "$1.2M", "$500K" to numeric values
    fn parse_currency_value(&self, value: &str) -> f64 {
        let clean_value = value.trim_start_matches('$').replace(",", "");

        if clean_value.ends_with('M') || clean_value.ends_with('m') {
            clean_value
                .trim_end_matches(['M', 'm'])
                .parse::<f64>()
                .unwrap_or(0.0)
                * 1_000_000.0
        } else if clean_value.ends_with('K') || clean_value.ends_with('k') {
            clean_value
                .trim_end_matches(['K', 'k'])
                .parse::<f64>()
                .unwrap_or(0.0)
                * 1_000.0
        } else if clean_value.ends_with('B') || clean_value.ends_with('b') {
            clean_value
                .trim_end_matches(['B', 'b'])
                .parse::<f64>()
                .unwrap_or(0.0)
                * 1_000_000_000.0
        } else {
            clean_value.parse::<f64>().unwrap_or(0.0)
        }
    }

    /// Convert server data to token structure (primary conversion method)
    fn convert_server_data_to_token(
        &self,
        token_data: &serde_json::Value,
        chain: &str,
    ) -> Option<DexScreenerTrendingToken> {
        Some(DexScreenerTrendingToken {
            address: token_data.get("address")?.as_str()?.to_string(),
            symbol: token_data.get("symbol")?.as_str()?.to_string(),
            name: token_data.get("name")?.as_str()?.to_string(),
            decimals: token_data
                .get("decimals")
                .and_then(|d| d.as_u64())
                .map(|d| d as u8),
            price: token_data
                .get("price")
                .and_then(|p| p.as_f64())
                .unwrap_or(0.0),
            price_change_24h: token_data.get("priceChange24h").and_then(|pc| pc.as_f64()),
            volume_24h: token_data.get("volume24h").and_then(|v| v.as_f64()),
            volume_change_24h: None, // Not available in server data
            liquidity: token_data.get("liquidity").and_then(|l| l.as_f64()),
            fdv: token_data.get("fdv").and_then(|f| f.as_f64()),
            marketcap: token_data.get("marketcap").and_then(|mc| mc.as_f64()),
            rank: token_data
                .get("rank")
                .and_then(|r| r.as_u64())
                .map(|r| r as u32),
            logo_uri: token_data
                .get("logoUri")
                .and_then(|l| l.as_str())
                .map(|s| s.to_string()),
            txns_24h: token_data
                .get("txns24h")
                .and_then(|t| t.as_u64())
                .map(|t| t as u32),
            last_trade_unix_time: None, // Not readily available
            chain_id: chain.to_string(),
            pair_address: token_data
                .get("pairAddress")
                .and_then(|pa| pa.as_str())
                .map(|s| s.to_string()),
        })
    }

    /// Convert DexScreener pair data to our token structure (legacy method - kept for compatibility)
    #[allow(dead_code)]
    fn convert_pair_to_token(
        &self,
        pair: &serde_json::Value,
        chain: &str,
    ) -> Option<DexScreenerTrendingToken> {
        let base_token = pair.get("baseToken")?;
        let price_usd = pair
            .get("priceUsd")
            .and_then(|p| p.as_str())
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(0.0);
        let volume_24h = pair
            .get("volume")
            .and_then(|v| v.get("h24"))
            .and_then(|h| h.as_f64());
        let price_change_24h = pair
            .get("priceChange")
            .and_then(|pc| pc.get("h24"))
            .and_then(|h| h.as_f64());
        let liquidity_usd = pair
            .get("liquidity")
            .and_then(|l| l.get("usd"))
            .and_then(|u| u.as_f64());
        let fdv = pair.get("fdv").and_then(|f| f.as_f64());
        let market_cap = pair.get("marketCap").and_then(|mc| mc.as_f64());
        let txns_24h = pair.get("txns").and_then(|t| t.get("h24")).and_then(|h| {
            let buys = h.get("buys").and_then(|b| b.as_u64()).unwrap_or(0);
            let sells = h.get("sells").and_then(|s| s.as_u64()).unwrap_or(0);
            Some((buys + sells) as u32)
        });

        Some(DexScreenerTrendingToken {
            address: base_token.get("address")?.as_str()?.to_string(),
            symbol: base_token.get("symbol")?.as_str()?.to_string(),
            name: base_token.get("name")?.as_str()?.to_string(),
            decimals: base_token
                .get("decimals")
                .and_then(|d| d.as_u64())
                .map(|d| d as u8),
            price: price_usd,
            price_change_24h,
            volume_24h,
            volume_change_24h: None, // Not available in DexScreener
            liquidity: liquidity_usd,
            fdv,
            marketcap: market_cap,
            rank: None, // Could be derived from position in array
            logo_uri: base_token
                .get("logoURI")
                .and_then(|l| l.as_str())
                .map(|s| s.to_string()),
            txns_24h,
            last_trade_unix_time: None, // Not readily available
            chain_id: chain.to_string(),
            pair_address: pair
                .get("pairAddress")
                .and_then(|pa| pa.as_str())
                .map(|s| s.to_string()),
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
            _ => "ethereum", // fallback to ethereum for unknown chains
        }
    }

    /// Wait for Cloudflare challenge to resolve
    async fn wait_for_cloudflare_challenge(
        &self,
        page: &Page,
        url: &str,
    ) -> Result<(), DexScreenerError> {
        // Check if we're on Cloudflare challenge page
        if let Ok(Some(title)) = page.get_title().await {
            if title.contains("Just a moment") || title.contains("Cloudflare") {
                info!(
                    "🔒 Cloudflare challenge detected (title: '{}'), waiting for resolution...",
                    title
                );

                // Poll for up to 30 seconds (15 attempts × 2 seconds)
                for attempt in 1..=15 {
                    tokio::time::sleep(Duration::from_secs(2)).await;

                    if let Ok(Some(new_title)) = page.get_title().await {
                        if !new_title.contains("Just a moment") && !new_title.contains("Cloudflare")
                        {
                            info!(
                                "✅ Cloudflare challenge resolved after {}s! New title: {}",
                                attempt * 2,
                                new_title
                            );
                            return Ok(());
                        }
                        debug!(
                            "⏳ Still on Cloudflare challenge page (attempt {}/15): {}",
                            attempt, new_title
                        );
                    }
                }

                // Still stuck after 30 seconds - log full HTML for debugging
                if let Ok(html) = page.content().await {
                    let html_preview = &html[..html.len().min(2000)];
                    error!(
                        "❌ Stuck on Cloudflare challenge after 30s for URL: {}\nHTML preview:\n{}",
                        url, html_preview
                    );
                }

                return Err(DexScreenerError::BrowserError(
                    "Cloudflare challenge not resolved after 30 seconds".to_string(),
                ));
            }
        }

        Ok(())
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

            page.evaluate(scroll_script.as_str())
                .await
                .map_err(|e| DexScreenerError::BrowserError(format!("Failed to scroll: {}", e)))?;

            // Random delay between scrolls
            tokio::time::sleep(Duration::from_millis(scroll_delay)).await;
        }

        // Scroll back to top
        page.evaluate("window.scrollTo(0, 0)").await.map_err(|e| {
            DexScreenerError::BrowserError(format!("Failed to scroll to top: {}", e))
        })?;

        debug!("Human behavior simulation completed");
        Ok(())
    }

    /// Extract trending tokens with retry logic (mirrored from working PoC)
    async fn extract_trending_tokens_with_retry(
        &self,
        page: &Page,
        chain: &str,
    ) -> Result<Vec<DexScreenerTrendingToken>, DexScreenerError> {
        let mut retries = 0;
        let max_retries = 10;

        loop {
            retries += 1;
            debug!(
                "Checking for server data... (attempt {}/{})",
                retries, max_retries
            );

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
            debug!(
                "Waiting for data to load... (attempt {}/{}) - {} ms",
                retries, max_retries, retry_delay
            );
            tokio::time::sleep(Duration::from_millis(retry_delay)).await;
        }
    }

    /// Get top traders for a specific token by scraping DexScreener
    /// Extracts trader wallet addresses from block explorer links in the "Top Traders" tab
    pub async fn get_top_traders_scraped(
        &mut self,
        token_address: &str,
        chain: &str,
    ) -> Result<Vec<TopTrader>, DexScreenerError> {
        if !self.config.enabled {
            return Ok(vec![]);
        }

        let browser = self.ensure_browser().await?;
        let page = browser
            .new_page("about:blank")
            .await
            .map_err(|e| DexScreenerError::BrowserError(format!("Failed to create page: {}", e)))?;

        // Configure anti-detection BEFORE navigation
        if self.config.anti_detection_enabled {
            self.setup_anti_detection(&page).await?;
        }

        // Build DexScreener token page URL
        let chain_path = self.get_dexscreener_chain_path(chain);
        let url = format!("https://dexscreener.com/{}/{}", chain_path, token_address);

        debug!(
            "🔍 Navigating to DexScreener token page for top traders: {}",
            url
        );

        // Add random delay before navigation
        let pre_nav_delay = {
            let mut rng = rand::thread_rng();
            rng.gen_range(500..1500)
        };
        tokio::time::sleep(Duration::from_millis(pre_nav_delay)).await;

        // Navigate with timeout
        let nav_result = tokio::time::timeout(Duration::from_secs(60), page.goto(&url)).await;

        match nav_result {
            Ok(Ok(_)) => {
                debug!("✅ Successfully navigated to token page: {}", url);
            }
            Ok(Err(e)) => {
                return Err(DexScreenerError::BrowserError(format!(
                    "Navigation failed: {}",
                    e
                )));
            }
            Err(_) => {
                return Err(DexScreenerError::BrowserError(format!(
                    "Navigation timed out after 60 seconds: {}",
                    url
                )));
            }
        }

        // Wait for page to load
        let load_delay = {
            let mut rng = rand::thread_rng();
            rng.gen_range(2000..4000)
        };
        tokio::time::sleep(Duration::from_millis(load_delay)).await;

        // Simulate human behavior
        self.simulate_human_behavior(&page).await?;

        // Look for and click the "Top Traders" button
        debug!("🎯 Looking for Top Traders button...");

        let top_traders_clicked = tokio::time::timeout(Duration::from_secs(15), async {
            // Try multiple selectors for the Top Traders button
            let selectors = [
                "button:has-text('Top Traders')",
                "button.chakra-button.custom-165cjlo",
                "button[type='button']:has-text('Top Traders')",
                "button:contains('Top Traders')",
                "//button[contains(text(), 'Top Traders')]",
            ];

            for selector in &selectors {
                debug!("Trying Top Traders button selector: {}", selector);

                // Use JavaScript to find and click the button containing "Top Traders"
                let click_result = page.evaluate(
                    r#"
                    () => {
                        // Find all buttons on the page
                        const buttons = Array.from(document.querySelectorAll('button'));

                        // Look for button containing "Top Traders" text
                        const topTradersButton = buttons.find(btn =>
                            btn.textContent && btn.textContent.includes('Top Traders')
                        );

                        if (topTradersButton) {
                            topTradersButton.click();
                            return 'clicked';
                        }
                        return 'not_found';
                    }
                    "#
                ).await;

                match click_result {
                    Ok(result) => {
                        if let Ok(value) = result.into_value::<String>() {
                            if value == "clicked" {
                                debug!("✅ Successfully clicked Top Traders button");
                                return Ok(());
                            }
                        }
                    }
                    Err(e) => {
                        debug!("❌ Failed to click Top Traders button with selector {}: {}", selector, e);
                    }
                }
            }

            Err(DexScreenerError::BrowserError("Top Traders button not found".to_string()))
        })
        .await;

        match top_traders_clicked {
            Ok(Ok(_)) => {
                debug!("✅ Top Traders button clicked successfully");
            }
            Ok(Err(e)) => {
                warn!("❌ Could not click Top Traders button: {}", e);
                return Ok(vec![]); // Return empty vec instead of error for graceful degradation
            }
            Err(_) => {
                warn!("⏰ Timeout waiting for Top Traders button");
                return Ok(vec![]);
            }
        }

        // Wait for top traders table to load after clicking
        let table_load_delay = {
            let mut rng = rand::thread_rng();
            rng.gen_range(3000..5000)
        };
        debug!("⏳ Waiting {} ms for top traders table to load...", table_load_delay);
        tokio::time::sleep(Duration::from_millis(table_load_delay)).await;

        // Extract block explorer links from the top traders table
        debug!("🔍 Extracting trader addresses from block explorer links...");

        let trader_addresses: Result<Result<Vec<String>, _>, _> = tokio::time::timeout(Duration::from_secs(20), async {
            let extraction_result = page.evaluate(
                r#"
                () => {
                    // Find all links that appear to be block explorer links
                    const explorerLinks = Array.from(document.querySelectorAll('a[href]'))
                        .filter(link => {
                            const href = link.href;
                            return href.includes('scan.io') ||
                                   href.includes('scan.com') ||
                                   href.includes('scan.org') ||
                                   href.includes('etherscan.io') ||
                                   href.includes('bscscan.com') ||
                                   href.includes('basescan.org') ||
                                   href.includes('solscan.io');
                        })
                        .map(link => link.href);

                    return explorerLinks;
                }
                "#
            ).await;

            match extraction_result {
                Ok(result) => {
                    if let Ok(value) = result.into_value() {
                        if let Ok(links) = serde_json::from_value::<Vec<String>>(value) {
                            return Ok::<Vec<String>, Box<dyn std::error::Error>>(links);
                        }
                    }
                }
                Err(e) => {
                    warn!("❌ Failed to extract block explorer links: {}", e);
                }
            }

            Ok::<Vec<String>, Box<dyn std::error::Error>>(vec![])
        })
        .await;

        let explorer_links = match trader_addresses {
            Ok(Ok(links)) => links,
            Ok(Err(_)) | Err(_) => {
                warn!("⏰ Timeout or error extracting trader addresses");
                return Ok(vec![]);
            }
        };

        debug!("🔗 Found {} block explorer links", explorer_links.len());

        // Parse wallet addresses from the explorer URLs
        let mut top_traders = Vec::new();
        let mut seen_addresses = std::collections::HashSet::new();

        for link in explorer_links.iter().take(15) { // Take up to 15 links to get ~10 unique traders
            let wallet_address = self.extract_wallet_address_from_url(link, chain);

            if let Some(address) = wallet_address {
                // Only add unique addresses
                if seen_addresses.insert(address.clone()) {
                    let trader = TopTrader {
                        token_address: token_address.to_string(),
                        owner: address,
                        tags: vec![],
                        trader_type: "24h".to_string(),
                        volume: 0.0, // Placeholder - no volume data from scraping
                        trade: 0,    // Placeholder
                        trade_buy: 0,    // Placeholder
                        trade_sell: 0,   // Placeholder
                        volume_buy: 0.0,  // Placeholder
                        volume_sell: 0.0, // Placeholder
                    };

                    top_traders.push(trader);

                    // Limit to 10 traders maximum
                    if top_traders.len() >= 10 {
                        break;
                    }
                }
            }
        }

        info!(
            "✅ Successfully scraped {} top traders for token {} on {}",
            top_traders.len(),
            token_address,
            chain
        );

        if self.config.headless_mode == false && !top_traders.is_empty() {
            debug!("🎯 Top traders found:");
            for (i, trader) in top_traders.iter().enumerate().take(5) {
                debug!("  {}. {}", i + 1, trader.owner);
            }
        }

        Ok(top_traders)
    }

    /// Extract wallet address from block explorer URL based on chain
    fn extract_wallet_address_from_url(&self, url: &str, chain: &str) -> Option<String> {
        use regex::Regex;

        let address_regex = match chain.to_lowercase().as_str() {
            "solana" => {
                // Solana addresses: alphanumeric, 32-44 characters
                Regex::new(r"solscan\.io/account/([A-Za-z0-9]{32,44})").ok()?
            }
            "ethereum" => {
                // Ethereum addresses: 0x followed by 40 hex characters
                Regex::new(r"etherscan\.io/address/(0x[a-fA-F0-9]{40})").ok()?
            }
            "bsc" | "binance-smart-chain" => {
                // BSC addresses: same format as Ethereum
                Regex::new(r"bscscan\.com/address/(0x[a-fA-F0-9]{40})").ok()?
            }
            "base" => {
                // Base addresses: same format as Ethereum
                Regex::new(r"basescan\.org/address/(0x[a-fA-F0-9]{40})").ok()?
            }
            _ => {
                warn!("⚠️ Unknown chain for address extraction: {}", chain);
                return None;
            }
        };

        if let Some(captures) = address_regex.captures(url) {
            if let Some(address_match) = captures.get(1) {
                let address = address_match.as_str().to_string();
                debug!("📍 Extracted wallet address: {} from URL: {}", address, url);
                return Some(address);
            }
        }

        debug!("❌ Could not extract address from URL: {}", url);
        None
    }
}
