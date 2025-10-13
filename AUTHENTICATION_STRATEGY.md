# Authentication Strategy for P&L Tracker API

## Overview

This document outlines the authentication strategy for securing the P&L Tracker API using Supabase JWT tokens.

## Current State

- **No authentication** - All endpoints are publicly accessible
- Empty `middleware.rs` file ready for implementation
- Axum router with permissive CORS configuration
- Frontend using Supabase for user authentication

## Supabase JWT Authentication Flow

### How It Works

1. **User Login (Frontend)**
   - User authenticates via Supabase (email/password, OAuth, magic link, etc.)
   - Supabase returns a JWT access token and refresh token
   - Token stored in frontend (localStorage, cookies, etc.)

2. **API Request (Frontend → Backend)**
   - Frontend includes token in HTTP header:
     ```
     Authorization: Bearer <jwt-token>
     ```

3. **Token Validation (Backend)**
   - Axum middleware extracts token from header
   - Validates JWT signature using Supabase JWT secret
   - Checks token expiration
   - Extracts user claims (user_id, email, role)
   - Adds user info to request context

4. **Request Processing**
   - Protected handlers access authenticated user info
   - User actions logged with user_id
   - User-specific business logic applied

### JWT Structure

Supabase JWTs contain:
```json
{
  "sub": "user-uuid",           // User ID
  "email": "user@example.com",
  "role": "authenticated",      // User role
  "aud": "authenticated",
  "exp": 1234567890,           // Expiration timestamp
  "iss": "https://your-project.supabase.co/auth/v1"
}
```

## Implementation Plan

### Phase 1: Dependencies

Add to `api_server/Cargo.toml`:
```toml
[dependencies]
jsonwebtoken = "9.2"  # JWT validation
base64 = "0.21"       # For decoding secrets
```

### Phase 2: Configuration

Add to `config_manager/src/lib.rs`:
```rust
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SupabaseConfig {
    /// JWT secret for validating tokens (from Supabase dashboard)
    pub jwt_secret: String,

    /// Supabase project URL
    pub project_url: String,

    /// Enable authentication (can disable for development)
    pub enable_auth: bool,
}
```

Add to `config.toml`:
```toml
[supabase]
jwt_secret = "your-supabase-jwt-secret"  # Get from Supabase Settings > API
project_url = "https://your-project.supabase.co"
enable_auth = true  # Set to false for local development
```

**Where to find JWT secret:**
- Supabase Dashboard → Settings → API → JWT Settings → `JWT Secret`

### Phase 3: Authentication Middleware

Create in `api_server/src/middleware.rs`:

```rust
use axum::{
    body::Body,
    extract::Request,
    http::{HeaderMap, StatusCode},
    middleware::Next,
    response::Response,
};
use jsonwebtoken::{decode, DecodingKey, Validation, Algorithm};
use serde::{Deserialize, Serialize};

/// JWT claims structure from Supabase
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Claims {
    pub sub: String,           // User ID
    pub email: Option<String>,
    pub role: Option<String>,
    pub exp: usize,           // Expiration timestamp
    pub iss: Option<String>,  // Issuer
}

/// Authenticated user info extracted from JWT
#[derive(Debug, Clone)]
pub struct AuthUser {
    pub user_id: String,
    pub email: Option<String>,
    pub role: Option<String>,
}

/// Authentication middleware
pub async fn auth_middleware(
    headers: HeaderMap,
    mut request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Extract token from Authorization header
    let token = extract_token(&headers)?;

    // Validate JWT and extract claims
    let claims = validate_jwt(&token)?;

    // Create AuthUser from claims
    let user = AuthUser {
        user_id: claims.sub,
        email: claims.email,
        role: claims.role,
    };

    // Add user to request extensions
    request.extensions_mut().insert(user);

    Ok(next.run(request).await)
}

fn extract_token(headers: &HeaderMap) -> Result<String, StatusCode> {
    let auth_header = headers
        .get("Authorization")
        .ok_or(StatusCode::UNAUTHORIZED)?
        .to_str()
        .map_err(|_| StatusCode::UNAUTHORIZED)?;

    if !auth_header.starts_with("Bearer ") {
        return Err(StatusCode::UNAUTHORIZED);
    }

    Ok(auth_header[7..].to_string())
}

fn validate_jwt(token: &str) -> Result<Claims, StatusCode> {
    // Get JWT secret from config (passed via app state or env)
    let jwt_secret = std::env::var("SUPABASE_JWT_SECRET")
        .map_err(|_| StatusCode::INTERNAL_SERVER_ERROR)?;

    let decoding_key = DecodingKey::from_secret(jwt_secret.as_bytes());
    let mut validation = Validation::new(Algorithm::HS256);
    validation.set_audience(&["authenticated"]);

    decode::<Claims>(token, &decoding_key, &validation)
        .map(|data| data.claims)
        .map_err(|_| StatusCode::UNAUTHORIZED)
}
```

### Phase 4: Apply Middleware to Routes

Update `api_server/src/main.rs`:

```rust
use axum::middleware::from_fn;
use crate::middleware::auth_middleware;

async fn create_router(state: AppState) -> Router {
    // Public routes (no auth required)
    let public_routes = Router::new()
        .route("/health", get(health_check))
        .route("/health/detailed", get(enhanced_health_check));

    // Protected routes (auth required)
    let protected_routes = Router::new()
        // Service management
        .route("/api/services/status", get(get_services_status))
        .route("/api/services/config", get(get_services_config))
        .route("/api/services/config", post(update_services_config))
        .route("/api/services/control", post(control_service))
        .route("/api/services/discovery/start", post(start_wallet_discovery))
        .route("/api/services/discovery/stop", post(stop_wallet_discovery))
        .route("/api/services/pnl/start", post(start_pnl_analysis))
        .route("/api/services/pnl/stop", post(stop_pnl_analysis))

        // Batch P&L
        .route("/api/pnl/batch/run", post(submit_batch_job))
        .route("/api/pnl/batch/status/:job_id", get(get_batch_job_status))
        .route("/api/pnl/batch/results/:job_id", get(get_batch_job_results))

        // Results management
        .route("/api/results", get(get_all_results))
        .route("/api/results/:wallet_address/favorite", post(toggle_wallet_favorite))
        .route("/api/results/:wallet_address/archive", post(toggle_wallet_archive))

        // Configuration
        .route("/api/config", post(update_config))

        // Apply authentication middleware
        .layer(from_fn(auth_middleware));

    // Combine routes
    Router::new()
        .merge(public_routes)
        .merge(protected_routes)
        .layer(
            ServiceBuilder::new()
                .layer(CorsLayer::permissive())
                .into_inner(),
        )
        .with_state(state)
}
```

### Phase 5: Access User Info in Handlers

Example handler using authenticated user:

```rust
use axum::Extension;
use crate::middleware::AuthUser;

pub async fn submit_batch_job(
    State(state): State<AppState>,
    Extension(user): Extension<AuthUser>,  // Authenticated user
    Json(request): Json<BatchJobRequest>,
) -> Result<Json<ApiResponse<BatchJobSubmission>>, ApiError> {
    info!("User {} submitting batch job for {} wallets",
          user.user_id, request.wallet_addresses.len());

    // Submit job with user tracking
    let job_id = state.orchestrator.submit_batch_job(
        request.wallet_addresses,
        request.chain,
        request.time_range,
        request.max_transactions,
    ).await?;

    // Optionally: Store user_id with job in database
    state.persistence_client.link_job_to_user(&job_id, &user.user_id).await?;

    Ok(Json(ApiResponse::success(BatchJobSubmission {
        job_id,
        wallet_count: request.wallet_addresses.len(),
        status: "Pending".to_string(),
        submitted_at: chrono::Utc::now(),
        submitted_by: Some(user.user_id),  // Track who submitted
    })))
}
```

## Security Considerations

### 1. JWT Secret Management
- **NEVER** commit JWT secret to git
- Store in environment variables or secure vault
- Use different secrets for dev/staging/production

### 2. Token Expiration
- Supabase tokens expire (default: 1 hour)
- Frontend must handle refresh token flow
- Backend rejects expired tokens automatically

### 3. HTTPS Only
- JWT tokens must be transmitted over HTTPS only
- Configure CORS to allow only trusted origins
- Use secure cookies for token storage (if applicable)

### 4. Rate Limiting
- Consider adding rate limiting per user
- Prevent abuse of expensive endpoints
- Track usage per user_id

### 5. Role-Based Access Control (Future)
- Supabase supports custom roles
- Can implement admin vs. user endpoints
- Example: Only admins can start/stop services

## Frontend Integration

### 1. Login and Store Token

```typescript
import { createClient } from '@supabase/supabase-js'

const supabase = createClient(
  'https://your-project.supabase.co',
  'your-anon-key'
)

// Login
const { data, error } = await supabase.auth.signInWithPassword({
  email: 'user@example.com',
  password: 'password'
})

if (data.session) {
  const token = data.session.access_token
  // Store token for API calls
  localStorage.setItem('supabase_token', token)
}
```

### 2. Make Authenticated API Calls

```typescript
const token = localStorage.getItem('supabase_token')

const response = await fetch('http://api.example.com/api/pnl/batch/run', {
  method: 'POST',
  headers: {
    'Content-Type': 'application/json',
    'Authorization': `Bearer ${token}`
  },
  body: JSON.stringify({
    wallet_addresses: ['wallet1', 'wallet2'],
    chain: 'solana'
  })
})
```

### 3. Handle Token Refresh

```typescript
// Check if token is expired and refresh
supabase.auth.onAuthStateChange((event, session) => {
  if (event === 'TOKEN_REFRESHED' && session) {
    localStorage.setItem('supabase_token', session.access_token)
  }
})
```

## Development Workflow

### Local Development (No Auth)
```toml
# config.toml
[supabase]
enable_auth = false  # Disable auth for local dev
```

### Testing with Auth
```bash
# Get token from Supabase dashboard or login flow
export SUPABASE_JWT_SECRET="your-secret"

# Test authenticated endpoint
curl -H "Authorization: Bearer <token>" \
     http://localhost:8080/api/pnl/batch/run
```

### Production Deployment
1. Set `enable_auth = true` in production config
2. Set `SUPABASE_JWT_SECRET` environment variable
3. Ensure HTTPS is enabled
4. Configure CORS for production frontend domain

## Benefits of This Approach

1. **Stateless** - No session storage needed, scales horizontally
2. **Fast** - JWT validation is cryptographic, no database calls
3. **Secure** - Industry-standard Bearer token authentication
4. **Supabase-native** - Leverages existing auth infrastructure
5. **User tracking** - Know who submitted each job
6. **Simple** - ~150 lines of middleware code
7. **Flexible** - Easy to add role-based access control later

## Future Enhancements

### 1. Rate Limiting per User
```rust
// Track requests per user_id
async fn rate_limit_middleware(
    Extension(user): Extension<AuthUser>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Check Redis for user's request count
    // Return 429 if over limit
}
```

### 2. Role-Based Access Control
```rust
// Only admins can control services
async fn admin_only_middleware(
    Extension(user): Extension<AuthUser>,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    if user.role != Some("admin".to_string()) {
        return Err(StatusCode::FORBIDDEN);
    }
    Ok(next.run(request).await)
}
```

### 3. API Key Support (for programmatic access)
```rust
// Support both JWT and API keys
async fn auth_middleware_flexible(
    headers: HeaderMap,
    request: Request<Body>,
    next: Next,
) -> Result<Response, StatusCode> {
    // Try JWT first
    if let Ok(user) = validate_jwt(&headers) {
        return Ok(user);
    }

    // Fall back to API key
    if let Ok(user) = validate_api_key(&headers) {
        return Ok(user);
    }

    Err(StatusCode::UNAUTHORIZED)
}
```

## Implementation Complexity

**Estimated Effort:** 2-3 hours

**Breakdown:**
- Add dependencies: 5 minutes
- Update config structure: 15 minutes
- Write middleware: 1 hour
- Apply to routes: 30 minutes
- Update handlers: 30 minutes
- Testing: 30 minutes

**Complexity Rating:** ⭐⭐ (Simple to Moderate)

The JWT validation pattern is well-established and Axum's middleware system makes it straightforward to implement.
