use crate::{config, help, json_rpc, media};

use anyhow::Result;
use clap::{ArgMatches, Args, FromArgMatches};
use serde_json::json;

#[derive(Args)]
pub struct CallArgs {
    /// 要调用的工具方法名
    #[arg(value_name = "method")]
    pub method: Option<String>,

    /// JSON 格式的参数
    #[arg(value_name = "args")]
    pub args: Option<String>,

    #[arg(long, short)]
    pub help: bool,
}

/// Handle the `call` subcommand: dispatch a JSON-RPC tool invocation for a given category and method.
pub async fn handle_call_cmd(category_name: &str, matches: &ArgMatches) -> Result<()> {
    let args = CallArgs::from_arg_matches(matches)?;

    // Check if the category is valid
    let categories = config::get_categories();
    if !categories.iter().any(|c| c.name == category_name) {
        anyhow::bail!("无效命令：{}", category_name);
    }

    if args.help {
        if let Some(method) = args.method.as_deref() {
            help::show_tool_help(category_name, method).await?;
        } else {
            help::show_category_tools(category_name).await?;
        }
        return Ok(());
    }

    // Get positional arg: method
    let Some(method) = args.method.as_deref() else {
        // No method provided, show category tools list
        help::show_category_tools(category_name).await?;
        return Ok(());
    };

    // Get positional arg: json_args (optional)
    let args = args.args.as_deref();

    // If no arguments provided, show tool help information
    if args.is_none() {
        help::show_tool_help(category_name, method).await?;
        return Ok(());
    }

    let timeout_ms = if method == "get_msg_media" {
        Some(120000)
    } else {
        None
    };

    let parsed_args = if let Some(args) = args {
        serde_json::from_str(args)?
    } else {
        json!({})
    };

    let params = json!({
        "name": method,
        "arguments": parsed_args,
    });

    let mut res = json_rpc::send(category_name, "tools/call", Some(params), timeout_ms).await?;

    if method == "get_msg_media" {
        res = media::intercept_media_response(res).await?;
    }

    if let Some(result) = res.get("result") {
        println!("{}", result);
    }

    Ok(())
}
