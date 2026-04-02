#!/usr/bin/env python3
"""
自动配置 wecom-cli
"""

import subprocess
import sys

BOT_ID = "aib1vJZb5yM73TZ69cv2Um_6nTojJxo6xt9"
BOT_SECRET = "HTb14rQ6WftiFuSRglmCNNUIGewwPtfPhJBVyGvE6j6"

print("🔧 正在配置 wecom-cli...")
print(f"Bot ID: {BOT_ID}")
print(f"Bot Secret: {BOT_SECRET[:10]}...")

# 使用 subprocess 传递输入
try:
    process = subprocess.Popen(
        ['wecom-cli', 'init'],
        stdin=subprocess.PIPE,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True
    )

    # 发送 Bot ID 和 Secret
    stdout, stderr = process.communicate(input=f"{BOT_ID}\n{BOT_SECRET}\n")

    print("STDOUT:")
    print(stdout)

    if stderr:
        print("STDERR:")
        print(stderr)

    print(f"返回码: {process.returncode}")

except Exception as e:
    print(f"❌ 错误: {e}")
    sys.exit(1)
