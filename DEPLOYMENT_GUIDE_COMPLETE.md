# P&L Tracker - Complete Digital Ocean Deployment Guide

This document provides a step-by-step guide for deploying the P&L Tracker system to a Digital Ocean droplet, from initial connection to full production deployment.

## 🎯 Deployment Overview

**Objective:** Deploy Rust-based P&L Tracker with BirdEye API integration to Digital Ocean  
**Target Environment:** Debian 12 droplet (2 CPU, 4GB RAM)  
**Final Result:** Production-ready API server at `http://134.199.211.155:8080`  

## 📋 Prerequisites

- Digital Ocean droplet (Debian 12)
- SSH key added to the droplet
- Local development environment with working P&L Tracker code
- BirdEye API credentials

## 🔧 Step-by-Step Deployment Process

### Step 1: Initial Connection and Environment Setup

#### 1.1 Test SSH Connection
```bash
# Get droplet details from Digital Ocean dashboard
# Public IP: 134.199.211.155
# Private IP: 10.124.0.2
# Username: root (default for Digital Ocean Debian droplets)

# Test connection
ssh -o StrictHostKeyChecking=no root@134.199.211.155 "echo 'Connected successfully' && uname -a"
```

**Expected Output:**
```
Connected successfully
Linux wallet-analyzer 6.1.0-26-amd64 #1 SMP PREEMPT_DYNAMIC Debian 6.1.112-1 (2024-09-30) x86_64 GNU/Linux
```

#### 1.2 System Update and Package Installation
```bash
# Update system packages
ssh root@134.199.211.155 "apt update && apt upgrade -y"

# Install essential dependencies
ssh root@134.199.211.155 "apt install -y curl git build-essential pkg-config libssl-dev redis-server"
```

**What this installs:**
- `curl`: For downloading and API testing
- `git`: Version control (if needed)
- `build-essential`: GCC, make, and build tools
- `pkg-config`: Package configuration tool
- `libssl-dev`: SSL development libraries for Rust
- `redis-server`: Redis database for caching and queuing

### Step 2: Rust Installation

#### 2.1 Install Rust Toolchain
```bash
# Install Rust using rustup
ssh root@134.199.211.155 "curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y"
```

**Expected Output:**
```
Rust is installed now. Great!
info: default toolchain set to 'stable-x86_64-unknown-linux-gnu'
```

#### 2.2 Verify Rust Installation
```bash
# Verify installation
ssh root@134.199.211.155 "source ~/.cargo/env && rustc --version && cargo --version"
```

**Expected Output:**
```
rustc 1.87.0 (17067e9ac 2025-05-09)
cargo 1.87.0 (99624be96 2025-05-06)
```

### Step 3: Redis Configuration

#### 3.1 Enable and Start Redis
```bash
# Enable Redis to start on boot and start it now
ssh root@134.199.211.155 "systemctl enable redis-server && systemctl start redis-server && systemctl status redis-server"
```

**Expected Status:**
```
● redis-server.service - Advanced key-value store
     Loaded: loaded (/lib/systemd/system/redis-server.service; enabled; preset: enabled)
     Active: active (running) since Fri 2025-06-20 10:41:44 UTC
```

#### 3.2 Test Redis Connection
```bash
# Test Redis connectivity
ssh root@134.199.211.155 "redis-cli ping"
```

**Expected Output:**
```
PONG
```

### Step 4: Project Code Deployment

#### 4.1 Create Project Directory
```bash
# Create project directory on server
ssh root@134.199.211.155 "mkdir -p /opt/pnl_tracker"
```

#### 4.2 Transfer Project Files
```bash
# From your local development machine, run this command in the project root:
tar -czf - --exclude='target' --exclude='.git' . | ssh root@134.199.211.155 "cd /opt/pnl_tracker && tar -xzf -"
```

**What this does:**
- Creates a compressed archive of the project
- Excludes build artifacts (`target/`) and git history (`.git/`)
- Pipes the archive to the server and extracts it

#### 4.3 Verify File Transfer
```bash
# Verify files were copied correctly
ssh root@134.199.211.155 "cd /opt/pnl_tracker && ls -la"
```

**Expected Files:**
```
drwxr-xr-x 13 1000 1000    4096 Jun 20 10:31 .
-rw-r--r--  1 1000 1000    2104 Jun 19 18:19 config.toml
-rw-r--r--  1 1000 1000     940 Jun 20 07:21 Cargo.toml
drwxr-xr-x  3 1000 1000    4096 Jun 17 23:43 api_server
drwxr-xr-x  3 1000 1000    4096 Jun 17 23:43 config_manager
[... other project directories and files ...]
```

### Step 5: Build the Application

#### 5.1 Build Release Version
```bash
# Build the project in release mode (optimized)
ssh root@134.199.211.155 "source ~/.cargo/env && cd /opt/pnl_tracker && cargo build --release"
```

**Build Process:**
- Downloads and compiles all dependencies
- Takes ~3-5 minutes depending on server performance
- Creates optimized binaries in `target/release/`

#### 5.2 Build API Server Specifically
```bash
# Build the API server binary
ssh root@134.199.211.155 "source ~/.cargo/env && cd /opt/pnl_tracker && cargo build --release -p api_server"
```

#### 5.3 Verify Binary Creation
```bash
# Check that the API server binary was created
ssh root@134.199.211.155 "ls -la /opt/pnl_tracker/target/release/api_server"
```

**Expected Output:**
```
-rwxr-xr-x 2 root root 13538800 Jun 20 11:00 /opt/pnl_tracker/target/release/api_server
```

### Step 6: Production Configuration

#### 6.1 Create Production Config File
```bash
# Create production configuration
ssh root@134.199.211.155 "cat > /opt/pnl_tracker/config.prod.toml << 'EOF'
[system]
debug_mode = false
redis_mode = true
process_loop_ms = 30000
output_csv_file = \"pnl_results.csv\"

[redis]
url = \"redis://localhost:6379\"
connection_timeout_seconds = 10
default_lock_ttl_seconds = 600

[dexscreener]
api_base_url = \"https://api.dexscreener.com\"
websocket_url = \"wss://io.dexscreener.com/dex/screener/v5/pairs/h24/1\"
http_base_url = \"https://io.dexscreener.com/dex/log/amm/v4/pumpfundex/top/solana\"
user_agent = \"Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36\"
reconnect_delay_seconds = 30
max_reconnect_attempts = 5

[dexscreener.trending]
min_volume_24h = 1270000.0
min_txns_24h = 45000
min_liquidity_usd = 10000.0
min_price_change_24h = 50.0
max_pair_age_hours = 168
polling_interval_seconds = 60
max_tokens_per_cycle = 20
wallet_discovery_limit = 10
rate_limit_ms = 200

[jupiter]
api_url = \"https://lite-api.jup.ag\"
price_cache_ttl_seconds = 60
request_timeout_seconds = 30

[birdeye]
api_key = \"5ff313b239ac42e297b830b10ea1871d\"
api_base_url = \"https://public-api.birdeye.so\"
request_timeout_seconds = 30
price_cache_ttl_seconds = 60
rate_limit_per_second = 100

[pnl]
timeframe_mode = \"none\"
timeframe_general = \"7d\"
wallet_min_capital = 0.0
aggregator_min_hold_minutes = 0.0
amount_trades = 0
win_rate = 0.0
aggregator_batch_size = 20

[trader_filter]
min_realized_pnl_usd = 100.0
min_total_trades = 5
min_winning_trades = 2
min_win_rate = 40.0
min_roi_percentage = 10.0
min_capital_deployed_sol = 1.0
max_avg_hold_time_minutes = 1440
min_avg_hold_time_minutes = 5
exclude_holders_only = true
exclude_zero_pnl = true
min_transaction_frequency = 0.1

[api]
host = \"0.0.0.0\"
port = 8080
enable_cors = true
request_timeout_seconds = 30
EOF"
```

**Key Production Settings:**
- `debug_mode = false`: Optimized logging for production
- `host = \"0.0.0.0\"`: Accept connections from any IP
- `port = 8080`: Standard HTTP alternative port
- Stricter trader filters for better quality results

### Step 7: Systemd Service Configuration

#### 7.1 Create Service File
```bash
# Create systemd service for auto-startup and management
ssh root@134.199.211.155 "cat > /etc/systemd/system/pnl-tracker.service << 'EOF'
[Unit]
Description=P&L Tracker API Server
After=network.target redis-server.service
Requires=redis-server.service

[Service]
Type=simple
User=root
WorkingDirectory=/opt/pnl_tracker
Environment=RUST_LOG=info
Environment=CONFIG_FILE=config.prod.toml
ExecStart=/opt/pnl_tracker/target/release/api_server
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal

# Resource limits
LimitNOFILE=65536
LimitNPROC=32768

[Install]
WantedBy=multi-user.target
EOF"
```

**Service Configuration Explained:**
- `After=network.target redis-server.service`: Start after network and Redis
- `Requires=redis-server.service`: Ensure Redis is running
- `Restart=always`: Auto-restart on failure
- `RestartSec=10`: Wait 10 seconds before restart
- `StandardOutput=journal`: Log to systemd journal

#### 7.2 Enable and Configure Service
```bash
# Reload systemd configuration and enable service
ssh root@134.199.211.155 "systemctl daemon-reload && systemctl enable pnl-tracker"
```

**Expected Output:**
```
Created symlink /etc/systemd/system/multi-user.target.wants/pnl-tracker.service → /etc/systemd/system/pnl-tracker.service.
```

### Step 8: Service Startup and Testing

#### 8.1 Start the Service
```bash
# Start the service and check status
ssh root@134.199.211.155 "systemctl start pnl-tracker && sleep 5 && systemctl status pnl-tracker"
```

**Expected Status:**
```
● pnl-tracker.service - P&L Tracker API Server
     Loaded: loaded (/lib/systemd/system/pnl-tracker.service; enabled; preset: enabled)
     Active: active (running) since Fri 2025-06-20 11:04:00 UTC; 5s ago
   Main PID: 45848 (api_server)
      Tasks: 4 (limit: 4653)
     Memory: 4.0M
        CPU: 51ms
```

#### 8.2 Test External Connectivity
```bash
# Test health endpoint from outside the server
curl -s http://134.199.211.155:8080/health
```

**Expected Response:**
```json
{
  "data": {
    "status": "healthy",
    "version": "0.1.0",
    "uptime_seconds": 0
  },
  "timestamp": "2025-06-20T11:04:26.607727788Z"
}
```

#### 8.3 Test Service Status Endpoint
```bash
# Test service status endpoint
curl -s http://134.199.211.155:8080/api/services/status
```

**Expected Response:**
```json
{
  "data": {
    "wallet_discovery": {
      "state": "Stopped",
      "discovered_wallets_total": 0,
      "queue_size": 0,
      "last_cycle_wallets": 0,
      "cycles_completed": 0,
      "last_activity": null
    },
    "pnl_analysis": {
      "state": "Stopped",
      "wallets_processed": 0,
      "wallets_in_progress": 0,
      "successful_analyses": 0,
      "failed_analyses": 0,
      "last_activity": null
    }
  },
  "timestamp": "2025-06-20T11:04:35.352417568Z"
}
```

### Step 9: End-to-End Functionality Testing

#### 9.1 Test Batch P&L Analysis
```bash
# Submit a batch job with a test wallet
curl -s -X POST http://134.199.211.155:8080/api/pnl/batch/run \
-H "Content-Type: application/json" \
-d '{
  "wallet_addresses": ["MfDuWeqSHEqTFVYZ7LoexgAK9dxk7cy4DFJWjWMGVWa"],
  "filters": {
    "min_capital_sol": "0",
    "min_hold_minutes": "0",
    "min_trades": 0,
    "min_win_rate": "0",
    "max_signatures": 1000
  }
}'
```

**Expected Response:**
```json
{
  "data": {
    "job_id": "9a83e283-fc03-4923-bdaf-7e9663e2f1ac",
    "wallet_count": 1,
    "status": "Pending",
    "submitted_at": "2025-06-20T11:05:08.281675488Z"
  },
  "timestamp": "2025-06-20T11:05:08.281675558Z"
}
```

#### 9.2 Check Job Status
```bash
# Check job status (wait 10 seconds for processing)
sleep 10
curl -s "http://134.199.211.155:8080/api/pnl/batch/status/9a83e283-fc03-4923-bdaf-7e9663e2f1ac"
```

**Expected Response:**
```json
{
  "data": {
    "job_id": "9a83e283-fc03-4923-bdaf-7e9663e2f1ac",
    "status": "Completed",
    "wallet_count": 1,
    "created_at": "2025-06-20T11:05:08.281655228Z",
    "started_at": "2025-06-20T11:05:08.281724868Z",
    "completed_at": "2025-06-20T11:05:10.882880099Z",
    "progress": {
      "total_wallets": 1,
      "completed_wallets": 1,
      "successful_wallets": 1,
      "failed_wallets": 0,
      "progress_percentage": 100.0
    }
  },
  "timestamp": "2025-06-20T11:05:40.832830761Z"
}
```

#### 9.3 Get Results Summary
```bash
# Get results summary
curl -s "http://134.199.211.155:8080/api/pnl/batch/results/9a83e283-fc03-4923-bdaf-7e9663e2f1ac" | jq '.data.summary'
```

**Expected Response:**
```json
{
  "total_wallets": 1,
  "successful_analyses": 1,
  "failed_analyses": 0,
  "total_pnl_usd": "9372.455610672622196566871383",
  "average_pnl_usd": "9372.455610672622196566871383",
  "profitable_wallets": 1
}
```

### Step 10: Monitoring and Maintenance Setup

#### 10.1 Install Additional Tools
```bash
# Install jq for JSON formatting in status scripts
ssh root@134.199.211.155 "apt install -y jq"
```

#### 10.2 Create Status Check Script
```bash
# Create comprehensive status check script
ssh root@134.199.211.155 "cat > /opt/pnl_tracker/status_check.sh << 'EOF'
#!/bin/bash

echo \"🔍 P&L Tracker System Status Check\"
echo \"=================================\"
echo

# Check systemd service
echo \"📊 Service Status:\"
systemctl status pnl-tracker --no-pager -l
echo

# Check if port is listening
echo \"🌐 Network Status:\"
ss -tlnp | grep :8080 || echo \"❌ Port 8080 not listening\"
echo

# Check Redis
echo \"💾 Redis Status:\"
redis-cli ping || echo \"❌ Redis not responding\"
echo

# Check API health
echo \"🩺 API Health Check:\"
curl -s http://localhost:8080/health | jq . || echo \"❌ API not responding\"
echo

# Check recent logs
echo \"📝 Recent Logs (last 5):\"
journalctl -u pnl-tracker -n 5 --no-pager
EOF

chmod +x /opt/pnl_tracker/status_check.sh"
```

#### 10.3 Test Status Script
```bash
# Test the status check script
ssh root@134.199.211.155 "/opt/pnl_tracker/status_check.sh"
```

## 📊 Deployment Verification Checklist

### ✅ System Services
- [ ] Redis server running and responding to PING
- [ ] P&L Tracker service active and enabled
- [ ] Service auto-restart configured
- [ ] Port 8080 accessible externally

### ✅ API Functionality
- [ ] Health endpoint responding correctly
- [ ] Service status endpoint working
- [ ] Batch P&L analysis completing successfully
- [ ] Job status tracking working
- [ ] Results retrieval working
- [ ] CSV export functioning

### ✅ Performance Metrics
- [ ] Memory usage reasonable (~5MB)
- [ ] Processing time acceptable (~3 seconds per wallet)
- [ ] No memory leaks observed
- [ ] CPU usage minimal when idle

### ✅ Data Processing
- [ ] BirdEye API integration working
- [ ] Large number handling functioning (u128 support)
- [ ] Embedded price extraction working
- [ ] No external price API calls being made
- [ ] Transaction parsing accurate

## 🚨 Troubleshooting Guide

### Common Issues and Solutions

#### 1. Service Won't Start
```bash
# Check service status and logs
systemctl status pnl-tracker
journalctl -u pnl-tracker -n 20

# Common fixes:
# - Ensure Redis is running: systemctl start redis-server
# - Check config file syntax: cd /opt/pnl_tracker && cargo check
# - Verify binary exists: ls -la target/release/api_server
```

#### 2. API Not Accessible Externally
```bash
# Check if service is listening on correct interface
ss -tlnp | grep 8080

# Should show: 0.0.0.0:8080 (not 127.0.0.1:8080)
# If wrong, check config.prod.toml [api] host setting
```

#### 3. Build Failures
```bash
# Ensure Rust is properly sourced
source ~/.cargo/env

# Check for missing dependencies
apt install build-essential pkg-config libssl-dev

# Clean and rebuild
cd /opt/pnl_tracker
cargo clean
cargo build --release
```

#### 4. Redis Connection Issues
```bash
# Test Redis connectivity
redis-cli ping

# If failed, restart Redis
systemctl restart redis-server
systemctl status redis-server
```

### Log Analysis
```bash
# View real-time logs
journalctl -u pnl-tracker -f

# View logs from specific time
journalctl -u pnl-tracker --since "2025-06-20 10:00:00"

# View error logs only
journalctl -u pnl-tracker -p err
```

## 📈 Performance Optimization

### Resource Monitoring
```bash
# Monitor system resources
htop
free -h
df -h

# Monitor service-specific resources
systemctl status pnl-tracker
```

### Scaling Considerations
- **Horizontal Scaling:** Deploy multiple droplets behind a load balancer
- **Vertical Scaling:** Upgrade to larger droplet for more CPU/RAM
- **Database Scaling:** Use Redis cluster for high availability
- **Caching:** Implement additional caching layers if needed

## 🔐 Security Considerations

### Production Security Hardening
```bash
# Create dedicated user (optional)
useradd -r -s /bin/false pnl-tracker
chown -R pnl-tracker:pnl-tracker /opt/pnl_tracker

# Update service to run as dedicated user
# Edit /etc/systemd/system/pnl-tracker.service
# Change: User=pnl-tracker

# Configure firewall (if needed)
ufw allow 8080/tcp
ufw enable
```

### API Security
- Add authentication middleware if required
- Implement rate limiting for public endpoints
- Use HTTPS with SSL certificates (Let's Encrypt)
- Regularly update dependencies

## 📝 Maintenance Procedures

### Regular Maintenance
```bash
# Update system packages
apt update && apt upgrade

# Check service health
/opt/pnl_tracker/status_check.sh

# Monitor logs for errors
journalctl -u pnl-tracker --since yesterday | grep -i error

# Check disk space
df -h /opt/pnl_tracker
```

### Backup Procedures
```bash
# Backup configuration
cp /opt/pnl_tracker/config.prod.toml /backup/

# Backup Redis data (if needed)
redis-cli BGSAVE
cp /var/lib/redis/dump.rdb /backup/

# Backup application logs
journalctl -u pnl-tracker > /backup/pnl-tracker-logs.txt
```

## 🎯 Deployment Success Metrics

**Final Deployment Status: ✅ SUCCESSFUL**

- **Server:** `http://134.199.211.155:8080`
- **Build Time:** ~3 minutes
- **Deployment Time:** ~15 minutes total
- **Test Results:** All endpoints functional
- **Performance:** 100 transactions analyzed in ~3 seconds
- **Memory Usage:** ~5MB (highly efficient)
- **Uptime:** Auto-restart configured
- **Reliability:** Large number edge cases resolved

## 📞 Support and Contact

For technical issues:
1. Check service status: `/opt/pnl_tracker/status_check.sh`
2. Review logs: `journalctl -u pnl-tracker -f`
3. Restart service: `systemctl restart pnl-tracker`
4. Contact backend team with specific error messages

**API Documentation:** `/opt/pnl_tracker/API_ENDPOINTS_DOCUMENTATION.md`  
**Service Status:** `systemctl status pnl-tracker`  
**Health Check:** `curl http://134.199.211.155:8080/health`

---

*This deployment guide represents the complete process used to successfully deploy the P&L Tracker to production on June 20, 2025.*