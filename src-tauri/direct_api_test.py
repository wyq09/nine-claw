#!/usr/bin/env python3
"""
直接测试可能的API格式
"""

import requests
import json

def test_openai_format():
    """测试OpenAI兼容格式"""
    print("🧪 测试OpenAI兼容格式...")
    
    url = "https://chenyu.pro/v1/chat/completions"
    api_key = "***REMOVED***06b6479e"
    
    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json"
    }
    
    data = {
        "model": "Jackrong/Qwen3.5-27B-Claude-4.6-Opus-Reasoning-Distilled-v2",
        "messages": [
            {"role": "user", "content": "Hello, are you working?"}
        ],
        "max_tokens": 50,
        "temperature": 0.7
    }
    
    try:
        print(f"📤 发送请求到: {url}")
        print(f"📝 请求体: {json.dumps(data, ensure_ascii=False)}")
        
        response = requests.post(url, headers=headers, json=data, timeout=30)
        print(f"📨 响应状态: {response.status_code}")
        print(f"📋 响应头: {dict(response.headers)}")
        print(f"📄 响应内容: {response.text[:500]}")
        
        if response.status_code == 200:
            try:
                result = response.json()
                print("✅ 成功获取JSON响应!")
                print(f"📊 响应解析: {json.dumps(result, ensure_ascii=False, indent=2)}")
            except:
                print("⚠️  响应不是JSON格式")
        else:
            print(f"❌ 请求失败: {response.status_code}")
            
    except Exception as e:
        print(f"💥 请求异常: {e}")

def test_ollama_format():
    """测试Ollama格式"""
    print("\n🧪 测试Ollama格式...")
    
    url = "https://chenyu.pro/api/generate"
    api_key = "***REMOVED***06b6479e"
    
    headers = {
        "Authorization": f"Bearer {api_key}",
        "Content-Type": "application/json"
    }
    
    data = {
        "model": "Jackrong/Qwen3.5-27B-Claude-4.6-Opus-Reasoning-Distilled-v2",
        "prompt": "Hello, are you working?",
        "stream": False
    }
    
    try:
        print(f"📤 发送请求到: {url}")
        response = requests.post(url, headers=headers, json=data, timeout=30)
        print(f"📨 响应状态: {response.status_code}")
        print(f"📄 响应内容: {response.text[:500]}")
        
    except Exception as e:
        print(f"💥 请求异常: {e}")

def test_without_auth():
    """测试不包含授权头的请求"""
    print("\n🧪 测试不包含授权头的请求...")
    
    # 尝试几个可能的不需要auth的端点
    test_urls = [
        "https://chenyu.pro/api/v1/models",
        "https://chenyu.pro/v1/models",
        "https://chenyu.pro/api/models/list"
    ]
    
    for url in test_urls:
        try:
            print(f"\n📡 测试: {url}")
            response = requests.get(url, timeout=10)
            print(f"   状态: {response.status_code}")
            
            if response.status_code == 200:
                content_type = response.headers.get('content-type', '')
                if 'json' in content_type:
                    print(f"   ✅ JSON响应 (可能成功)")
                    try:
                        data = response.json()
                        print(f"   数据: {json.dumps(data, ensure_ascii=False)[:200]}...")
                    except:
                        print(f"   内容: {response.text[:200]}...")
                else:
                    print(f"   ⚠️  非JSON响应: {content_type}")
                    if response.text.startswith('{') or response.text.startswith('['):
                        print(f"   可能是JSON: {response.text[:200]}...")
            else:
                print(f"   ❌ 失败: {response.status_code}")
                
        except Exception as e:
            print(f"   💥 错误: {e}")

def check_domain_info():
    """检查域名信息"""
    print("\n🔍 检查域名信息...")
    
    try:
        # 获取主页
        response = requests.get("https://chenyu.pro", timeout=10)
        print(f"主页状态: {response.status_code}")
        
        if response.status_code == 200:
            # 检查标题
            html = response.text
            if "<title>" in html:
                start = html.find("<title>") + 7
                end = html.find("</title>", start)
                title = html[start:end] if end > start else "未找到"
                print(f"页面标题: {title}")
            else:
                print("未找到页面标题")
                
    except Exception as e:
        print(f"主页检查失败: {e}")

def main():
    print("🔬 直接API格式测试")
    print("=" * 60)
    
    check_domain_info()
    test_without_auth()
    test_openai_format()
    test_ollama_format()
    
    print("\n" + "=" * 60)
    print("📝 测试完成！")
    print("\n💡 可能的结论:")
    print("1. API端点路径可能不正确")
    print("2. API服务可能需要特定的请求格式")
    print("3. 可能需要WebSocket或其他协议")
    print("4. 服务可能已更改或停止")

if __name__ == "__main__":
    main()