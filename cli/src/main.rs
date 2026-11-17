use clap::{Parser, Subcommand};
use labyrinth_cli::{Client, Config, Error, Keychain};
use reqwest::Method;
use serde_json::json;
use std::io::{BufRead, Read, Write};
use uuid::Uuid;

#[derive(Parser)]
#[command(
    name = "laby",
    version,
    about = "Labyrinth hosted network engineering client"
)]
struct Args {
    #[arg(long, env = "LABY_API_URL", global = true)]
    api_url: Option<String>,
    #[arg(long, env = "LABY_SUPABASE_URL", global = true)]
    supabase_url: Option<String>,
    #[arg(
        long,
        env = "LABY_SUPABASE_PUBLISHABLE_KEY",
        global = true,
        hide_env_values = true
    )]
    publishable_key: Option<String>,
    #[command(subcommand)]
    command: Command,
}
#[derive(Subcommand)]
enum Command {
    /// Send a magic link and wait for the same-machine browser callback.
    Login {
        #[arg(long,default_value_t=54327,value_parser=clap::value_parser!(u16).range(1024..))]
        callback_port: u16,
    },
    /// Revoke this provider session and erase the saved credential.
    Logout,
    /// Read the current application identity without creating a workspace.
    Whoami,
    /// Explicitly create/retrieve your personal workspace.
    Onboard,
    /// List the first page of your current tenant memberships.
    Tenants,
    /// List the first page of active projects in a tenant.
    Projects { tenant: Uuid },
    /// Create a project; preserve the key if an outcome is uncertain.
    ProjectCreate {
        tenant: Uuid,
        name: String,
        #[arg(long)]
        idempotency_key: String,
    },
}
#[tokio::main]
async fn main() -> std::process::ExitCode {
    let args = Args::parse();
    match run(args).await {
        Ok(()) => std::process::ExitCode::SUCCESS,
        Err(error) => {
            eprintln!("{error}");
            std::process::ExitCode::FAILURE
        }
    }
}
async fn run(args: Args) -> Result<(), Error> {
    let config = Config::new(
        args.api_url.as_deref().ok_or(Error::InvalidInput)?,
        args.supabase_url.as_deref().ok_or(Error::InvalidInput)?,
        args.publishable_key.as_deref().ok_or(Error::InvalidInput)?,
    )?;
    let vault = Keychain::open(&config)?;
    let client = Client::new(config)?;
    let (method, path, body, key) = match args.command {
        Command::Login { callback_port } => {
            eprint!("Email: ");
            std::io::stderr().flush().map_err(|_| Error::InvalidInput)?;
            let mut email = String::new();
            std::io::stdin()
                .lock()
                .take(322)
                .read_line(&mut email)
                .map_err(|_| Error::InvalidInput)?;
            let email = email.trim_end_matches(['\n', '\r']);
            eprintln!(
                "Requesting a magic link. Open it in a browser on this computer; keep this terminal running (five-minute limit). No workspace is created by login."
            );
            client.login(email, callback_port, &vault).await?;
            println!(
                "Login saved in the OS credential store. Run laby whoami, then laby onboard when ready."
            );
            return Ok(());
        }
        Command::Logout => {
            client.logout(&vault).await?;
            println!("Logged out locally and provider session revoked.");
            return Ok(());
        }
        Command::Whoami => (Method::GET, "v1/me".into(), None, None),
        Command::Onboard => (
            Method::POST,
            "v1/onboarding/personal-workspace".into(),
            Some(json!({})),
            None,
        ),
        Command::Tenants => (Method::GET, "v1/tenants".into(), None, None),
        Command::Projects { tenant } => (
            Method::GET,
            format!("v1/tenants/{tenant}/projects"),
            None,
            None,
        ),
        Command::ProjectCreate {
            tenant,
            name,
            idempotency_key,
        } => {
            if name.is_empty()
                || name.chars().count() > 200
                || name.trim() != name
                || name.chars().any(char::is_control)
            {
                return Err(Error::InvalidInput);
            }
            (
                Method::POST,
                format!("v1/tenants/{tenant}/projects"),
                Some(json!({"name":name})),
                Some(idempotency_key),
            )
        }
    };
    let value = client
        .api(&vault, method, &path, body, key.as_deref())
        .await?;
    println!(
        "{}",
        serde_json::to_string_pretty(&value).map_err(|_| Error::InvalidResponse)?
    );
    Ok(())
}
