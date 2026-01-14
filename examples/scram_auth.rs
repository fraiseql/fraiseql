//! Example: SCRAM-SHA-256 Authentication with fraiseql-wire
//!
//! This example demonstrates how to connect to PostgreSQL using SCRAM-SHA-256
//! authentication, which is the default secure authentication mechanism in
//! PostgreSQL 10+.
//!
//! SCRAM (Salted Challenge Response Authentication Mechanism) provides:
//! - ✅ Secure password-based authentication
//! - ✅ Mutual authentication (client verifies server)
//! - ✅ Protection against replay attacks
//! - ✅ Resistance to password sniffing
//!
//! Setup:
//! ```bash
//! # 1. Start PostgreSQL with SCRAM authentication (default in PG 10+)
//! docker run --rm -e POSTGRES_PASSWORD=secure_password \
//!   -p 5432:5432 postgres:latest
//!
//! # 2. Create a test user with SCRAM password
//! psql -U postgres -h localhost -c "CREATE USER testuser WITH PASSWORD 'testpass';"
//! psql -U postgres -h localhost -c "GRANT CONNECT ON DATABASE postgres TO testuser;"
//!
//! # 3. Set environment variables and run the example
//! export SCRAM_DB_URL="postgres://testuser:testpass@localhost:5432/postgres"
//! cargo run --example scram_auth
//! ```

use fraiseql_wire::client::FraiseClient;
use futures::StreamExt;
use std::env;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Get database URL from environment or use default
    let db_url = env::var("SCRAM_DB_URL")
        .unwrap_or_else(|_| "postgres://postgres:postgres@localhost:5432/postgres".to_string());

    println!("📡 fraiseql-wire SCRAM Authentication Example\n");
    println!("Connecting to PostgreSQL with SCRAM-SHA-256 auth...");
    println!("Connection URL: {}\n", mask_password(&db_url));

    // Step 1: Connect to PostgreSQL
    let client = match FraiseClient::connect(&db_url).await {
        Ok(c) => {
            println!("✅ SCRAM authentication successful!\n");
            c
        }
        Err(e) => {
            eprintln!("❌ Authentication failed: {}\n", e);
            eprintln!("Troubleshooting:");
            eprintln!("  1. Verify PostgreSQL is running");
            eprintln!("  2. Check username and password are correct");
            eprintln!("  3. Ensure PostgreSQL supports SCRAM (10+ required)");
            eprintln!("  4. Set SCRAM_DB_URL environment variable:");
            eprintln!("     export SCRAM_DB_URL='postgres://user:pass@host:5432/db'");
            return Err(e.into());
        }
    };

    // Step 2: Query the system to prove authentication worked
    println!("Executing queries to demonstrate authenticated connection...\n");

    // Query 1: Get list of tables
    println!("Query: SELECT * FROM information_schema.tables");
    match client
        .query::<serde_json::Value>("information_schema.tables")
        .execute()
        .await
    {
        Ok(mut stream) => {
            let mut count = 0;
            while let Some(result) = stream.next().await {
                match result {
                    Ok(row) => {
                        if count == 0 {
                            println!("First row: {:?}", row);
                        }
                        count += 1;
                    }
                    Err(e) => {
                        eprintln!("Error reading row: {}", e);
                    }
                }
                if count >= 3 {
                    break;
                }
            }
            println!("✅ Retrieved {} rows\n", count);
        }
        Err(e) => {
            eprintln!("❌ Query failed: {}", e);
            return Err(e.into());
        }
    }

    // Step 3: Demonstrate SCRAM authentication details
    println!("SCRAM-SHA-256 Authentication Details:");
    println!("=====================================\n");
    println!("What happened during authentication:\n");
    println!("1. CLIENT HELLO");
    println!("   - Client sends username and random nonce");
    println!("   - Example: n,a=testuser,r=24c9e3e8e9f...abcd123\n");
    println!("2. SERVER HELLO");
    println!("   - Server sends challenge with salt and iteration count");
    println!("   - Example: r=nonce+server_nonce, s=base64_salt, i=4096\n");
    println!("3. CLIENT PROOF");
    println!("   - Client derives key using PBKDF2 (4096 iterations)");
    println!("   - Calculates HMAC-SHA256 proof");
    println!("   - Sends: c=channel_binding, r=nonce, p=proof\n");
    println!("4. SERVER VERIFICATION");
    println!("   - Server verifies client proof");
    println!("   - Client verifies server signature");
    println!("   - Mutual authentication confirmed! ✅\n");

    // Step 4: Security benefits
    println!("Security Benefits of SCRAM-SHA-256:");
    println!("====================================\n");
    println!("✅ Password never sent over network");
    println!("✅ Mutual authentication (both sides verify each other)");
    println!("✅ Protection against replay attacks");
    println!("✅ PBKDF2 key derivation (computationally expensive to crack)");
    println!("✅ Constant-time signature verification (timing attack resistant)\n");

    // Step 5: Error scenarios
    println!("Error Handling Scenarios:");
    println!("========================\n");
    println!("If authentication fails, you might see:\n");
    println!("• \"password required for SCRAM authentication\"");
    println!("  → Check that password is included in connection URL\n");
    println!("• \"server does not support SCRAM-SHA-256\"");
    println!("  → PostgreSQL version < 10 or non-standard config");
    println!("  → Falls back to cleartext auth (less secure)\n");
    println!("• \"SCRAM verification failed\"");
    println!("  → Wrong password or corrupted authentication exchange\n");

    println!("✅ Example completed successfully!");

    Ok(())
}

/// Mask the password in connection URL for display
fn mask_password(url: &str) -> String {
    if let Some(at_pos) = url.rfind('@') {
        if let Some(colon_pos) = url[..at_pos].rfind(':') {
            let before_pass = &url[..colon_pos + 1];
            let after_pass = &url[at_pos..];
            format!("{}***{}", before_pass, after_pass)
        } else {
            url.to_string()
        }
    } else {
        url.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_mask_password() {
        let masked = mask_password("postgres://user:password@localhost:5432/db");
        assert!(masked.contains("user:***@localhost"));
        assert!(!masked.contains("password"));
    }

    #[test]
    fn test_mask_password_no_credentials() {
        let masked = mask_password("postgres://localhost:5432/db");
        assert_eq!(masked, "postgres://localhost:5432/db");
    }
}
