#!/usr/bin/env python3
"""
手动创建 wecom-cli 配置文件
"""
import json
import os
from pathlib import Path

# 配置信息
CORP_ID = "wwa85b0bfcaa9698e4"
BOT_ID = "aibWgZBlwYLcIiqINUazRrnRYh3CwaG9bEN"
BOT_SECRET = "ugCs3g15eIHn9N4k95O5nsFXMU66lxyCGjX3bOBOffc"

# 创建配置目录
config_dir = Path.home() / ".config" / "wecom"
config_dir.mkdir(parents=True, exist_ok=True)

print("🔧 配置目录:", config_dir)

# 创建 bot 配置
bot_config = {
    "id": BOT_ID,
    "secret": BOT_SECRET
}

print("\n📝 Bot 配置信息:")
print(f"  Corp ID: {CORP_ID}")
print(f"  Bot ID: {BOT_ID}")
print(f"  Bot Secret: {BOT_SECRET[:10]}...")

# 注意：wecom-cli 使用加密存储，我们不能直接创建明文配置
# 但可以先创建一个简单的测试配置文件

config_file = config_dir / "config.json"
with open(config_file, 'w', encoding='utf-8') as f:
    json.dump({
        "corp_id": CORP_ID,
        "bot_id": BOT_ID,
        "bot_secret": BOT_SECRET
    }, f, indent=2, ensure_ascii=False)

print(f"\n✅ 配置文件已创建: {config_file}")
print("\n⚠️  注意: 这是临时配置文件，wecom-cli 使用加密存储")
print("正式配置需要通过 wecom-cli init 命令完成")

# 测试企业微信API连接
print("\n🔍 测试企业微信API连接...")
import urllib.request
import urllib.parse

# 构建请求URL
url = f"https://qyapi.weixin.qq.com/cgi-bin/gettoken?corpid={CORP_ID}&corpsecret={BOT_SECRET}"

try:
    with urllib.request.urlopen(url, timeout=10) as response:
        result = json.loads(response.read().decode('utf-8'))
        print(f"\n📊 API响应:")
        print(json.dumps(result, indent=2, ensure_ascii=False))

        if result.get('errcode') == 0:
            print("\n✅ 企业微信API连接成功！")
            print(f"Access Token: {result.get('access_token', '')[:20]}...")
        else:
            print(f"\n❌ 连接失败: {result.get('errmsg')}")
            print("\n可能的原因:")
            print("1. Corp ID 不正确")
            print("2. Secret 不是企业级别的Secret，而是应用级别的Secret")
            print("3. IP白名单限制")
except Exception as e:
    print(f"\n❌ 请求失败: {e}")
