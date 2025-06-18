# P&L Tracker Deployment Guide

## Quick Start

### Prerequisites
- Rust (latest stable) - Install from https://rustup.rs/
- Redis Server - For continuous mode functionality
- Git - For cloning the repository

### 1. Clone and Build
```bash
git clone <repository-url>
cd pnl_tracker

# Build in release mode for production
cargo build --release

# Or development mode
cargo build
```

### 2. Configure the System
```bash
# Copy example configuration
cp config.toml config_production.toml

# Edit configuration file
nano config_production.toml
```

**Key Configuration Sections:**

```toml
[system]
debug_mode = false        # Set to false for production
redis_mode = true         # Enable continuous mode
process_loop_ms = 30000   # Process interval in milliseconds

[api]
host = "0.0.0.0"         # Bind to all interfaces
port = 8080              # API server port
enable_cors = true       # Enable CORS for frontend

[redis]
url = "redis://localhost:6379"  # Redis connection string

[solana]
rpc_url = "https://your-solana-rpc-endpoint"  # Use your RPC provider
max_signatures = 50      # Transactions per wallet
rpc_timeout_seconds = 30

[jupiter]
api_url = "https://lite-api.jup.ag"  # Jupiter price API
```

### 3. Setup Redis (Ubuntu/Debian)
```bash
# Install Redis
sudo apt update
sudo apt install redis-server

# Start Redis service
sudo systemctl start redis-server
sudo systemctl enable redis-server

# Test Redis connection
redis-cli ping
```

### 4. Run the System
```bash
# Development mode
cargo run -p api_server

# Production mode (release build)
./target/release/api_server

# With custom config file
RUST_LOG=info ./target/release/api_server --config config_production.toml
```

---

## Production Deployment

### Using systemd (Recommended)

1. **Create systemd service file:**
```bash
sudo nano /etc/systemd/system/pnl-tracker.service
```

```ini
[Unit]
Description=P&L Tracker API Server
After=network.target redis.service
Wants=redis.service

[Service]
Type=simple
User=pnl-tracker
Group=pnl-tracker
WorkingDirectory=/opt/pnl-tracker
ExecStart=/opt/pnl-tracker/target/release/api_server
Restart=always
RestartSec=10
StandardOutput=journal
StandardError=journal
Environment=RUST_LOG=info,api_server=debug

[Install]
WantedBy=multi-user.target
```

2. **Create dedicated user:**
```bash
sudo useradd -r -s /bin/false pnl-tracker
sudo mkdir -p /opt/pnl-tracker
sudo chown pnl-tracker:pnl-tracker /opt/pnl-tracker
```

3. **Deploy application:**
```bash
# Copy files to production directory
sudo cp -r . /opt/pnl-tracker/
sudo chown -R pnl-tracker:pnl-tracker /opt/pnl-tracker

# Enable and start service
sudo systemctl daemon-reload
sudo systemctl enable pnl-tracker
sudo systemctl start pnl-tracker

# Check status
sudo systemctl status pnl-tracker
```

### Using Docker

1. **Create Dockerfile:**
```dockerfile
FROM rust:1.75 as builder

WORKDIR /app
COPY . .
RUN cargo build --release

FROM debian:bookworm-slim

RUN apt-get update && apt-get install -y \
    ca-certificates \
    && rm -rf /var/lib/apt/lists/*

WORKDIR /app

COPY --from=builder /app/target/release/api_server .
COPY config.toml .

EXPOSE 8080

CMD ["./api_server"]
```

2. **Create docker-compose.yml:**
```yaml
version: '3.8'

services:
  redis:
    image: redis:7-alpine
    restart: unless-stopped
    ports:
      - "6379:6379"
    volumes:
      - redis_data:/data

  pnl-tracker:
    build: .
    restart: unless-stopped
    ports:
      - "8080:8080"  
    depends_on:
      - redis
    environment:
      - RUST_LOG=info,api_server=debug
    volumes:
      - ./config.toml:/app/config.toml:ro

volumes:
  redis_data:
```

3. **Deploy with Docker:**
```bash
# Build and start
docker-compose up -d

# View logs
docker-compose logs -f pnl-tracker

# Update deployment
docker-compose pull
docker-compose up -d --build
```

---

## Configuration Reference

### Complete Configuration File
```toml
[system]
debug_mode = false
redis_mode = true
process_loop_ms = 30000
output_csv_file = "pnl_results.csv"

[redis]
url = "redis://localhost:6379"
connection_timeout_seconds = 10
default_lock_ttl_seconds = 600

[solana]
rpc_url = "https://solana-mainnet.g.alchemy.com/v2/YOUR_API_KEY"
max_signatures = 50
rpc_timeout_seconds = 30
max_concurrent_requests = 10

[dexscreener]
api_base_url = "https://api.dexscreener.com"
websocket_url = "wss://io.dexscreener.com/dex/screener/v5/pairs/h24/1"
http_base_url = "https://io.dexscreener.com/dex/log/amm/v4/pumpfundex/top/solana"
user_agent = "Mozilla/5.0 (X11; Linux x86_64) AppleWebKit/537.36"
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
api_url = "https://lite-api.jup.ag"
price_cache_ttl_seconds = 60
request_timeout_seconds = 30

[pnl]
timeframe_mode = "none"
timeframe_general = "7d"
wallet_min_capital = 0.0
aggregator_min_hold_minutes = 0.0
amount_trades = 0
win_rate = 0.0
aggregator_batch_size = 20

[trader_filter]
min_realized_pnl_usd = 100.0
min_total_trades = 5
min_winning_trades = 3
min_win_rate = 60.0
min_roi_percentage = 10.0
min_capital_deployed_sol = 1.0
max_avg_hold_time_minutes = 4320
min_avg_hold_time_minutes = 60
exclude_holders_only = true
exclude_zero_pnl = true
min_transaction_frequency = 0.1

[api]
host = "0.0.0.0"
port = 8080
enable_cors = true
request_timeout_seconds = 30
```

### Environment Variables
You can override configuration using environment variables:

```bash
export RUST_LOG=info
export PNL_REDIS_URL=redis://localhost:6379
export PNL_API_PORT=8080
export PNL_SOLANA_RPC_URL=https://your-rpc-endpoint
```

---

## Operational Modes

### Batch Mode Only
```toml
[system]
redis_mode = false
```
- Lighter resource usage
- Only API endpoints for batch analysis
- No continuous monitoring
- No Redis dependency

### Continuous Mode (24/7)
```toml
[system]
redis_mode = true
```
- Full feature set
- Automatic wallet discovery
- Background processing
- Requires Redis

---

## Monitoring and Maintenance

### Health Checks
```bash
# Basic health check
curl http://localhost:8080/health

# Detailed status
curl http://localhost:8080/api/status

# Check specific services
curl http://localhost:8080/api/dex/status
```

### Log Management
```bash
# View service logs (systemd)
sudo journalctl -u pnl-tracker -f

# Docker logs
docker-compose logs -f pnl-tracker

# Rotate logs (configure logrotate)
sudo nano /etc/logrotate.d/pnl-tracker
```

### Performance Monitoring
Monitor these metrics:
- Memory usage (Redis + API server)
- CPU usage during batch processing
- Network I/O for external API calls
- Redis queue lengths
- Response times

### Backup Strategy
Backup important data:
- Configuration files
- Redis data (if persistent)
- Generated CSV reports
- Application logs

---

## Troubleshooting

### Common Issues

**Port Already in Use:**
```bash
# Find process using port 8080
sudo lsof -i :8080
sudo kill -9 <PID>
```

**Redis Connection Failed:**
```bash
# Check Redis status
sudo systemctl status redis-server
redis-cli ping

# Restart Redis
sudo systemctl restart redis-server
```

**High Memory Usage:**
- Reduce `max_concurrent_requests` in Solana config
- Lower `aggregator_batch_size` in P&L config
- Decrease `wallet_discovery_limit` in DexScreener config

**External API Timeouts:**
- Increase timeout values in config
- Check network connectivity
- Verify API endpoints are accessible

### Debug Mode
Enable detailed logging:
```toml
[system]
debug_mode = true
```

Or use environment variable:
```bash
RUST_LOG=debug ./target/release/api_server
```

---

## Security Considerations

### Network Security
- Run behind reverse proxy (nginx/Apache)
- Use HTTPS in production
- Configure firewall to restrict access
- Consider API rate limiting

### Data Protection  
- Secure Redis instance
- Regular security updates
- Monitor for unusual API usage
- Backup encryption for sensitive data

### API Security
- Add authentication for sensitive endpoints
- Input validation and sanitization
- CORS configuration for frontend domains
- Request logging and monitoring

---

## Scaling Considerations

### Horizontal Scaling
- Multiple API server instances behind load balancer
- Shared Redis instance for coordination
- Database for persistent storage (future enhancement)

### Performance Optimization
- Increase `max_concurrent_requests` based on hardware
- Optimize batch sizes for your workload
- Consider Redis clustering for high throughput
- Monitor and tune garbage collection

### Resource Requirements

**Minimum (Development):**
- 2 CPU cores
- 4GB RAM
- 10GB storage

**Recommended (Production):**
- 4+ CPU cores  
- 8GB+ RAM
- 50GB+ storage
- SSD for Redis data

The system is now ready for production deployment and frontend integration!