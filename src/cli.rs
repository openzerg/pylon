use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "pylon")]
#[command(about = "Pylon - LLM API Gateway for OpenZerg")]
#[command(version)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(about = "Start the gateway server")]
    Serve {
        #[arg(short, long, default_value = "8080")]
        port: u16,
        #[arg(short = 'g', long, default_value = "50051")]
        grpc_port: u16,
    },

    #[command(about = "Proxy management")]
    Proxy {
        #[command(subcommand)]
        command: ProxyCommands,
    },

    #[command(about = "Permission management")]
    Permission {
        #[command(subcommand)]
        command: PermissionCommands,
    },

    #[command(about = "Model operations")]
    Model {
        #[command(subcommand)]
        command: ModelCommands,
    },

    #[command(about = "Log operations")]
    Logs {
        #[command(subcommand)]
        command: LogCommands,
    },

    #[command(about = "Test chat completion")]
    Chat {
        #[arg(short, long)]
        model: String,
        #[arg(short, long)]
        message: String,
        #[arg(short = 'a', long, default_value = "admin")]
        agent: String,
        #[arg(short, long, default_value = "http://localhost:8080")]
        url: String,
    },
}

#[derive(Subcommand)]
pub enum ProxyCommands {
    #[command(about = "List all proxies")]
    List,

    #[command(about = "Get proxy details")]
    Get {
        #[arg(short, long)]
        id: String,
    },

    #[command(about = "Create a new proxy")]
    Create {
        #[arg(short, long)]
        id: String,
        #[arg(short = 's', long)]
        source_model: String,
        #[arg(short = 't', long)]
        target_model: String,
        #[arg(short, long)]
        upstream: String,
        #[arg(short = 'k', long)]
        api_key: String,
        #[arg(short = 'm', long)]
        default_max_tokens: Option<i32>,
        #[arg(short = 'T', long)]
        default_temperature: Option<f64>,
        #[arg(long)]
        no_streaming: bool,
        #[arg(long)]
        support_tools: bool,
        #[arg(long)]
        support_vision: bool,
    },

    #[command(about = "Update an existing proxy")]
    Update {
        #[arg(short, long)]
        id: String,
        #[arg(short = 's', long)]
        source_model: Option<String>,
        #[arg(short = 't', long)]
        target_model: Option<String>,
        #[arg(short, long)]
        upstream: Option<String>,
        #[arg(short = 'k', long)]
        api_key: Option<String>,
        #[arg(short = 'm', long)]
        default_max_tokens: Option<i32>,
        #[arg(short = 'T', long)]
        default_temperature: Option<f64>,
    },

    #[command(about = "Delete a proxy")]
    Delete {
        #[arg(short, long)]
        id: String,
    },
}

#[derive(Subcommand)]
pub enum PermissionCommands {
    #[command(about = "Authorize an agent to use a proxy")]
    Authorize {
        #[arg(short, long)]
        proxy_id: String,
        #[arg(short = 'a', long)]
        agent: String,
        #[arg(short = 'l', long, default_value = "use")]
        level: String,
    },

    #[command(about = "Revoke an agent's permission")]
    Revoke {
        #[arg(short, long)]
        proxy_id: String,
        #[arg(short = 'a', long)]
        agent: String,
    },

    #[command(about = "List permissions for a proxy")]
    List {
        #[arg(short, long)]
        proxy_id: String,
    },

    #[command(about = "Check if agent has permission")]
    Check {
        #[arg(short, long)]
        proxy_id: String,
        #[arg(short = 'a', long)]
        agent: String,
    },
}

#[derive(Subcommand)]
pub enum ModelCommands {
    #[command(about = "List available models")]
    List,
}

#[derive(Subcommand)]
pub enum LogCommands {
    #[command(about = "Query request logs")]
    Query {
        #[arg(short = 'u', long)]
        user_id: Option<String>,
        #[arg(short, long)]
        proxy_id: Option<String>,
        #[arg(short = 'm', long)]
        model: Option<String>,
        #[arg(long)]
        success: Option<bool>,
        #[arg(short, long, default_value = "20")]
        limit: i32,
    },

    #[command(about = "Show statistics")]
    Stats,
}

pub async fn handle_command(cli: Cli) -> anyhow::Result<()> {
    match cli.command {
        Commands::Serve { port, grpc_port } => {
            crate::proxy::serve(port, grpc_port).await?;
        }
        Commands::Proxy { command } => {
            handle_proxy_command(command).await?;
        }
        Commands::Permission { command } => {
            handle_permission_command(command).await?;
        }
        Commands::Model { command } => {
            handle_model_command(command).await?;
        }
        Commands::Logs { command } => {
            handle_log_command(command).await?;
        }
        Commands::Chat { model, message, agent, url } => {
            handle_chat_command(model, message, agent, url).await?;
        }
    }
    Ok(())
}

async fn handle_proxy_command(command: ProxyCommands) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let base_url = std::env::var("PYLON_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());
    let token = create_admin_token();

    match command {
        ProxyCommands::List => {
            let resp = client
                .get(&format!("{}/v1/proxies", base_url))
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await?
                .json::<Vec<crate::db::Proxy>>()
                .await?;

            if resp.is_empty() {
                println!("No proxies found.");
            } else {
                println!("{:<20} {:<25} {:<25} {:<10}", "ID", "SOURCE MODEL", "TARGET MODEL", "STREAM");
                println!("{}", "-".repeat(80));
                for p in resp {
                    println!("{:<20} {:<25} {:<25} {:<10}", 
                        p.id, p.source_model, p.target_model, 
                        if p.support_streaming { "Yes" } else { "No" });
                }
            }
        }
        ProxyCommands::Get { id } => {
            let resp = client
                .get(&format!("{}/v1/proxies/{}", base_url, id))
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await?;

            if resp.status() == 404 {
                println!("Proxy '{}' not found.", id);
                return Ok(());
            }

            let proxy = resp.json::<crate::db::Proxy>().await?;
            println!("ID: {}", proxy.id);
            println!("Source Model: {}", proxy.source_model);
            println!("Target Model: {}", proxy.target_model);
            println!("Upstream: {}", proxy.upstream);
            println!("Streaming: {}", proxy.support_streaming);
            println!("Tools: {}", proxy.support_tools);
            println!("Vision: {}", proxy.support_vision);
            if let Some(t) = proxy.default_max_tokens {
                println!("Default Max Tokens: {}", t);
            }
            if let Some(t) = proxy.default_temperature {
                println!("Default Temperature: {}", t);
            }
            println!("Created: {}", proxy.created_at);
            println!("Updated: {}", proxy.updated_at);
        }
        ProxyCommands::Create { id, source_model, target_model, upstream, api_key, default_max_tokens, default_temperature, no_streaming, support_tools, support_vision } => {
            let now = chrono::Utc::now().to_rfc3339();
            let proxy = crate::db::Proxy {
                id,
                source_model,
                target_model,
                upstream,
                api_key,
                default_max_tokens,
                default_temperature,
                default_top_p: None,
                default_top_k: None,
                support_streaming: !no_streaming,
                support_tools,
                support_vision,
                extra_headers: None,
                extra_body: None,
                created_at: now.clone(),
                updated_at: now,
            };

            let resp = client
                .post(&format!("{}/v1/proxies", base_url))
                .header("Authorization", format!("Bearer {}", token))
                .json(&proxy)
                .send()
                .await?;

            if resp.status().is_success() {
                println!("Proxy '{}' created successfully.", proxy.id);
            } else {
                println!("Failed to create proxy: {}", resp.status());
            }
        }
        ProxyCommands::Update { id, source_model, target_model, upstream, api_key, default_max_tokens, default_temperature } => {
            let existing = client
                .get(&format!("{}/v1/proxies/{}", base_url, &id))
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await?
                .json::<crate::db::Proxy>()
                .await?;

            let now = chrono::Utc::now().to_rfc3339();
            let proxy = crate::db::Proxy {
                id: id.clone(),
                source_model: source_model.unwrap_or(existing.source_model),
                target_model: target_model.unwrap_or(existing.target_model),
                upstream: upstream.unwrap_or(existing.upstream),
                api_key: api_key.unwrap_or(existing.api_key),
                default_max_tokens: default_max_tokens.or(existing.default_max_tokens),
                default_temperature: default_temperature.or(existing.default_temperature),
                default_top_p: existing.default_top_p,
                default_top_k: existing.default_top_k,
                support_streaming: existing.support_streaming,
                support_tools: existing.support_tools,
                support_vision: existing.support_vision,
                extra_headers: existing.extra_headers,
                extra_body: existing.extra_body,
                created_at: existing.created_at,
                updated_at: now,
            };

            let resp = client
                .post(&format!("{}/v1/proxies/{}", base_url, id))
                .header("Authorization", format!("Bearer {}", token))
                .json(&proxy)
                .send()
                .await?;

            if resp.status().is_success() {
                println!("Proxy '{}' updated successfully.", proxy.id);
            } else {
                println!("Failed to update proxy: {}", resp.status());
            }
        }
        ProxyCommands::Delete { id } => {
            let resp = client
                .delete(&format!("{}/v1/proxies/{}", base_url, id))
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await?;

            if resp.status().is_success() {
                println!("Proxy '{}' deleted.", id);
            } else {
                println!("Failed to delete proxy: {}", resp.status());
            }
        }
    }
    Ok(())
}

async fn handle_permission_command(command: PermissionCommands) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let base_url = std::env::var("PYLON_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());
    let token = create_admin_token();

    match command {
        PermissionCommands::Authorize { proxy_id, agent, level } => {
            let resp = client
                .post(&format!("{}/v1/proxies/{}/authorize", base_url, proxy_id))
                .header("Authorization", format!("Bearer {}", token))
                .json(&serde_json::json!({ "agent_name": agent, "permission_level": level }))
                .send()
                .await?;

            if resp.status().is_success() {
                println!("Agent '{}' authorized for proxy '{}'.", agent, proxy_id);
            } else {
                println!("Failed to authorize: {}", resp.status());
            }
        }
        PermissionCommands::Revoke { proxy_id, agent } => {
            let resp = client
                .post(&format!("{}/v1/proxies/{}/revoke", base_url, proxy_id))
                .header("Authorization", format!("Bearer {}", token))
                .json(&serde_json::json!({ "agent_name": agent }))
                .send()
                .await?;

            if resp.status().is_success() {
                println!("Permission revoked for agent '{}' on proxy '{}'.", agent, proxy_id);
            } else {
                println!("Failed to revoke: {}", resp.status());
            }
        }
        PermissionCommands::List { proxy_id } => {
            let resp = client
                .get(&format!("{}/v1/proxies/{}/permissions", base_url, proxy_id))
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await?
                .json::<Vec<crate::db::Permission>>()
                .await?;

            if resp.is_empty() {
                println!("No permissions for proxy '{}'.", proxy_id);
            } else {
                println!("{:<20} {:<15} {:<20}", "AGENT", "LEVEL", "GRANTED BY");
                println!("{}", "-".repeat(55));
                for p in resp {
                    println!("{:<20} {:<15} {:<20}", p.agent_name, p.permission_level, p.granted_by);
                }
            }
        }
        PermissionCommands::Check { proxy_id, agent } => {
            let resp = client
                .get(&format!("{}/v1/proxies/{}/permissions", base_url, proxy_id))
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await?
                .json::<Vec<crate::db::Permission>>()
                .await?;

            let has_permission = resp.iter().any(|p| p.agent_name == agent);
            println!("Agent '{}' {} access to proxy '{}'.", 
                agent, 
                if has_permission { "HAS" } else { "DOES NOT HAVE" },
                proxy_id);
        }
    }
    Ok(())
}

async fn handle_model_command(command: ModelCommands) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let base_url = std::env::var("PYLON_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());
    let token = create_admin_token();

    match command {
        ModelCommands::List => {
            #[derive(serde::Deserialize)]
            struct ModelInfo {
                id: String,
            }
            #[derive(serde::Deserialize)]
            struct ModelsResponse {
                data: Vec<ModelInfo>,
            }

            let resp = client
                .get(&format!("{}/v1/models", base_url))
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await?
                .json::<ModelsResponse>()
                .await?;

            if resp.data.is_empty() {
                println!("No models available.");
            } else {
                println!("{:<30}", "MODEL ID");
                println!("{}", "-".repeat(30));
                for m in resp.data {
                    println!("{:<30}", m.id);
                }
            }
        }
    }
    Ok(())
}

async fn handle_log_command(command: LogCommands) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let base_url = std::env::var("PYLON_URL").unwrap_or_else(|_| "http://localhost:8080".to_string());
    let token = create_admin_token();

    match command {
        LogCommands::Query { user_id, proxy_id, model, success, limit } => {
            let mut url = format!("{}/v1/logs?limit={}", base_url, limit);
            if let Some(u) = user_id {
                url.push_str(&format!("&user_id={}", u));
            }
            if let Some(p) = proxy_id {
                url.push_str(&format!("&proxy_id={}", p));
            }
            if let Some(m) = model {
                url.push_str(&format!("&source_model={}", m));
            }
            if let Some(s) = success {
                url.push_str(&format!("&is_success={}", s));
            }

            let resp = client
                .get(&url)
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await?
                .json::<Vec<crate::db::RequestLog>>()
                .await?;

            if resp.is_empty() {
                println!("No logs found.");
            } else {
                println!("{:<20} {:<15} {:<20} {:<8} {:<10}", "TIME", "USER", "MODEL", "STATUS", "DURATION");
                println!("{}", "-".repeat(75));
                for log in resp {
                    let status = if log.is_success { "OK" } else { "ERR" };
                    let duration = format!("{}ms", log.duration_ms.unwrap_or(0));
                    println!("{:<20} {:<15} {:<20} {:<8} {:<10}", 
                        log.created_at.split('T').next().unwrap_or(&log.created_at),
                        log.user_id,
                        log.source_model,
                        status,
                        duration);
                }
            }
        }
        LogCommands::Stats => {
            let resp = client
                .get(&format!("{}/v1/logs/stats", base_url))
                .header("Authorization", format!("Bearer {}", token))
                .send()
                .await?
                .json::<crate::db::DashboardStats>()
                .await?;

            println!("=== Pylon Statistics ===");
            println!("Total Proxies: {}", resp.total_proxies);
            println!("Requests Today: {}", resp.total_requests_today);
            println!("Successful: {}", resp.successful_requests_today);
            println!("Success Rate: {:.1}%", resp.success_rate);
            println!("Avg Duration: {:.0}ms", resp.avg_duration_ms);
            println!("Total Input Tokens: {}", resp.total_input_tokens);
            println!("Total Output Tokens: {}", resp.total_output_tokens);
        }
    }
    Ok(())
}

async fn handle_chat_command(model: String, message: String, agent: String, url: String) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let token = create_agent_token(&agent);

    let body = serde_json::json!({
        "model": model,
        "messages": [{"role": "user", "content": message}],
        "stream": false
    });

    let resp = client
        .post(&format!("{}/v1/chat/completions", url))
        .header("Authorization", format!("Bearer {}", token))
        .json(&body)
        .send()
        .await?;

    let status = resp.status();
    if status.is_success() {
        let result = resp.json::<serde_json::Value>().await?;
        if let Some(content) = result["choices"][0]["message"]["content"].as_str() {
            println!("{}", content);
        } else {
            println!("{}", serde_json::to_string_pretty(&result)?);
        }
    } else {
        let error = resp.text().await?;
        println!("Error ({}): {}", status, error);
    }

    Ok(())
}

fn create_admin_token() -> String {
    let secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "IkqX8V9dTSqTvqyBDsMt9iaKZn50rfhKyGCizUpaEcRQqgdeZkXuW1J4ZC8WYyXIA1Imar07oZeFW+nlgG4Gmw==".to_string());
    
    let now = chrono::Utc::now().timestamp();
    let claims = crate::proxy::Claims {
        iss: "pylon-cli".to_string(),
        sub: "admin".to_string(),
        role: "admin".to_string(),
        iat: now,
        exp: now + 3600,
    };

    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    ).unwrap()
}

fn create_agent_token(agent: &str) -> String {
    let secret = std::env::var("JWT_SECRET")
        .unwrap_or_else(|_| "IkqX8V9dTSqTvqyBDsMt9iaKZn50rfhKyGCizUpaEcRQqgdeZkXuW1J4ZC8WYyXIA1Imar07oZeFW+nlgG4Gmw==".to_string());
    
    let now = chrono::Utc::now().timestamp();
    let claims = crate::proxy::Claims {
        iss: "pylon-cli".to_string(),
        sub: agent.to_string(),
        role: "agent".to_string(),
        iat: now,
        exp: now + 3600,
    };

    jsonwebtoken::encode(
        &jsonwebtoken::Header::default(),
        &claims,
        &jsonwebtoken::EncodingKey::from_secret(secret.as_bytes()),
    ).unwrap()
}