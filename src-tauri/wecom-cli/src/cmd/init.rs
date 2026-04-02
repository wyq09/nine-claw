use crate::auth;
use crate::mcp;
use anyhow::Result;
use clap::ArgMatches;
use clap::Args;
use clap::FromArgMatches;

#[derive(Args)]
pub struct InitArgs {
    #[arg(long, help = "企业微信机器人 Bot ID")]
    bot_id: Option<String>,

    #[arg(long, help = "仅刷新 MCP 后台配置")]
    refresh: bool,
}

/// Handle the `init` subcommand: prompt for bot credentials, persist them, and verify via MCP config fetch.
pub async fn handle_init_cmd(matches: &ArgMatches) -> Result<()> {
    let args = InitArgs::from_arg_matches(matches)?;

    if args.refresh {
        mcp::config::fetch_mcp_config().await?;
        println!("MCP 后台配置刷新成功");
        return Ok(());
    }

    cliclack::intro("企业微信机器人初始化")?;

    let bot_id: String = match args.bot_id {
        Some(id) => id,
        None => cliclack::input("企业微信机器人 Bot ID")
            .placeholder("请输入企业微信机器人ID")
            .interact()?,
    };

    let bot_secret: String = cliclack::password("企业微信机器人 Secret")
        .mask('*')
        .interact()?;

    let bot = auth::Bot::new(bot_id, bot_secret);
    auth::set_bot_info(&bot)?;

    // Verify credentials by fetching MCP config from server
    let spinner = cliclack::spinner();
    spinner.start("正在验证企业微信机器人凭证...");

    match mcp::config::fetch_mcp_config().await {
        Ok(_) => {
            spinner.stop("企业微信机器人凭证验证成功");
            cliclack::outro("初始化完成 ✅")?;
            Ok(())
        }
        Err(e) => {
            spinner.stop("企业微信机器人凭证验证失败");
            // Credentials invalid or server unreachable — rollback
            auth::clear_bot_info();
            mcp::config::clear_mcp_config();
            cliclack::outro("初始化失败 ❌")?;
            anyhow::bail!("验证企业微信机器人凭证失败: {e}");
        }
    }
}
