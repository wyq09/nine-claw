#!/usr/bin/env python3
"""
全面测试LLM API连接和功能
尝试不同的请求格式和参数
"""

import requests
import json
import time

# 配置参数
CONFIG = {
    "api_url": "https://chenyu.pro/api/v1/llm",
    "model": "Jackrong/Qwen3.5-27B-Claude-4.6-Opus-Reasoning-Distilled-v2",
    "api_key": "***REMOVED***06b6479e",
    "max_tokens": 256000
}

def print_header(title):
    """打印标题"""
    print(f"\n{'='*80}")
    print(f"🧪 {title}")
    print(f"{'='*80}")

def test_api_connection():
    """测试基本的API连接"""
    print_header("测试基础API连接")
    
    # 测试1: 简单GET请求
    print("📡 测试1: GET请求...")
    try:
        response = requests.get(CONFIG['api_url'], timeout=10)
        print(f"   GET响应: {response.status_code} - {response.text[:100]}")
    except Exception as e:
        print(f"   GET错误: {e}")
    
    # 测试2: 带认证的HEAD请求
    print("\n📡 测试2: HEAD请求（带认证）...")
    headers = {"Authorization": f"Bearer {CONFIG['api_key']}"}
    try:
        response = requests.head(CONFIG['api_url'], headers=headers, timeout=10)
        print(f"   HEAD响应: {response.status_code}")
        print(f"   响应头: {dict(response.headers)}")
    except Exception as e:
        print(f"   HEAD错误: {e}")

def test_format_openai():
    """测试OpenAI兼容格式"""
    print_header("测试OpenAI兼容格式")
    
    headers = {
        "Authorization": f"Bearer {CONFIG['api_key']}",
        "Content-Type": "application/json"
    }
    
    # 尝试多种OpenAI格式
    formats = [
        {
            "name": "标准ChatCompletion",
            "data": {
                "model": CONFIG['model'],
                "messages": [
                    {"role": "user", "content": "Hello, are you working? Please respond with 'API TEST SUCCESS' if you see this."}
                ],
                "max_tokens": 50,
                "temperature": 0.7
            }
        },
        {
            "name": "简单格式",
            "data": {
                "model": CONFIG['model'],
                "message": "Hello, are you working?",
                "max_tokens": 50
            }
        },
        {
            "name": "流式格式 (stream=False)",
            "data": {
                "model": CONFIG['model'],
                "messages": [
                    {"role": "user", "content": "Test streaming format"}
                ],
                "stream": False,
                "max_tokens": 30
            }
        },
        {
            "name": "流式格式 (stream=True)",
            "data": {
                "model": CONFIG['model'],
                "messages": [
                    {"role": "user", "content": "Test streaming"}
                ],
                "stream": True,
                "max_tokens": 30
            }
        }
    ]
    
    for format_item in formats:
        print(f"\n📤 测试格式: {format_item['name']}")
        print(f"   请求体: {json.dumps(format_item['data'], ensure_ascii=False)[:150]}...")
        
        try:
            start_time = time.time()
            response = requests.post(
                CONFIG['api_url'],
                headers=headers,
                json=format_item['data'],
                timeout=30,
                stream=format_item['data'].get('stream', False)
            )
            elapsed = time.time() - start_time
            
            print(f"   响应时间: {elapsed:.2f}秒")
            print(f"   状态码: {response.status_code}")
            print(f"   内容类型: {response.headers.get('content-type', '未知')}")
            
            if response.status_code == 200:
                if format_item['data'].get('stream', False):
                    content = ""
                    for line in response.iter_lines():
                        if line:
                            content += line.decode('utf-8') + "\n"
                        if len(content) > 200:
                            break
                    print(f"   流式响应预览: {content[:200]}...")
                else:
                    try:
                        data = response.json()
                        print(f"   ✅ JSON响应成功!")
                        print(f"       响应结构: {json.dumps(list(data.keys()) if isinstance(data, dict) else type(data), ensure_ascii=False)}")
                        # 尝试提取回复
                        if isinstance(data, dict):
                            # 尝试多种可能的回复字段
                            possible_keys = ['choices', 'response', 'content', 'text', 'message', 'answer']
                            for key in possible_keys:
                                if key in data:
                                    print(f"       找到字段 '{key}': {str(data[key])[:100]}...")
                                    break
                    except json.JSONDecodeError:
                        print(f"   ⚠️  响应不是JSON格式: {response.text[:200]}...")
            else:
                print(f"   ❌ HTTP错误: {response.text[:200]}...")
                
        except Exception as e:
            print(f"   💥 请求异常: {type(e).__name__}: {e}")

def test_format_alternative():
    """测试替代格式"""
    print_header("测试替代API格式")
    
    headers = {
        "Authorization": f"Bearer {CONFIG['api_key']}",
        "Content-Type": "application/json"
    }
    
    # 尝试其他可能的格式
    formats = [
        {
            "name": "简洁格式",
            "url": CONFIG['api_url'],
            "data": {
                "query": "What is 2+2?",
                "model": CONFIG['model']
            }
        },
        {
            "name": "提示工程格式",
            "url": CONFIG['api_url'],
            "data": {
                "prompt": "You are a helpful assistant. Question: What is the capital of France? Answer:",
                "model": CONFIG['model'],
                "max_tokens": 50
            }
        },
        {
            "name": "带系统消息",
            "url": CONFIG['api_url'],
            "data": {
                "model": CONFIG['model'],
                "messages": [
                    {"role": "system", "content": "You are a helpful assistant."},
                    {"role": "user", "content": "Say hello!"}
                ],
                "max_tokens": 30
            }
        }
    ]
    
    for format_item in formats:
        print(f"\n📤 测试: {format_item['name']}")
        
        try:
            response = requests.post(
                format_item['url'],
                headers=headers,
                json=format_item['data'],
                timeout=20
            )
            
            print(f"   状态码: {response.status_code}")
            
            if response.status_code == 200:
                try:
                    data = response.json()
                    print(f"   ✅ JSON响应成功!")
                    
                    # 简化显示
                    if isinstance(data, dict):
                        keys = list(data.keys())
                        print(f"       字段: {keys}")
                        
                        # 显示部分内容
                        for key in ['choices', 'response', 'content', 'answer']:
                            if key in data:
                                value = data[key]
                                if isinstance(value, list) and len(value) > 0:
                                    value = value[0]
                                if isinstance(value, dict):
                                    value = json.dumps(value, ensure_ascii=False)
                                print(f"       {key}: {str(value)[:100]}...")
                except:
                    print(f"   ⚠️  非JSON响应: {response.text[:200]}...")
            else:
                print(f"   ❌ 失败: {response.status_code}")
                print(f"   错误: {response.text[:200]}...")
                
        except Exception as e:
            print(f"   💥 异常: {e}")

def test_model_capabilities():
    """测试模型能力"""
    print_header("测试模型推理能力")
    
    if not CONFIG['api_url']:
        print("❌ 无法测试 - API端点无效")
        return
    
    headers = {
        "Authorization": f"Bearer {CONFIG['api_key']}",
        "Content-Type": "application/json"
    }
    
    test_cases = [
        {
            "name": "数学推理",
            "prompt": "If a car travels at 60 km/h for 2 hours, how far does it travel? Think step by step."
        },
        {
            "name": "代码生成", 
            "prompt": "Write a Python function to calculate factorial."
        },
        {
            "name": "逻辑推理",
            "prompt": "All men are mortal. Socrates is a man. Therefore, Socrates is mortal. Is this valid logic?"
        }
    ]
    
    for test in test_cases:
        print(f"\n🤔 测试: {test['name']}")
        
        data = {
            "model": CONFIG['model'],
            "messages": [
                {"role": "user", "content": test['prompt']}
            ],
            "max_tokens": 200,
            "temperature": 0.3
        }
        
        try:
            response = requests.post(
                CONFIG['api_url'],
                headers=headers,
                json=data,
                timeout=30
            )
            
            if response.status_code == 200:
                try:
                    result = response.json()
                    print(f"   ✅ 模型响应成功")
                    
                    # 提取回复
                    reply = "No response found"
                    if isinstance(result, dict):
                        # 尝试提取回复内容
                        if 'choices' in result and result['choices']:
                            choice = result['choices'][0]
                            if isinstance(choice, dict) and 'message' in choice:
                                reply = choice['message'].get('content', 'No content')
                            elif isinstance(choice, dict) and 'text' in choice:
                                reply = choice['text']
                        elif 'response' in result:
                            reply = result['response']
                        elif 'content' in result:
                            reply = result['content']
                    
                    print(f"   📝 回复: {reply[:150]}...")
                    
                except json.JSONDecodeError:
                    print(f"   ⚠️  响应不是JSON: {response.text[:200]}...")
            else:
                print(f"   ❌ 请求失败: {response.status_code}")
                
        except Exception as e:
            print(f"   💥 测试异常: {e}")

def test_claude_code_integration():
    """测试Claude Code集成可能性"""
    print_header("测试Claude Code集成配置")
    
    print("📝 Claude Code 集成可能的配置方式:")
    print("\n1. **作为OpenAI兼容API使用:**")
    print("   ```python")
    print(f'   base_url = "{CONFIG["api_url"]}"')
    print(f'   api_key = "{CONFIG["api_key"]}"')
    print(f'   model = "{CONFIG["model"]}"')
    print("   ```")
    
    print("\n2. **使用OpenAI SDK配置:**")
    print("   ```python")
    print("   from openai import OpenAI")
    print("   ")
    print(f'   client = OpenAI(')
    print(f'       base_url="{CONFIG["api_url"]}",')
    print(f'       api_key="{CONFIG["api_key"]}"')
    print("   )")
    print("   ")
    print("   response = client.chat.completions.create(")
    print(f'       model="{CONFIG["model"]}",')
    print("       messages=[")
    print('           {"role": "user", "content": "Hello"}')
    print("       ]")
    print("   )")
    print("   ```")
    
    print("\n3. **环境变量设置:**")
    print("   ```bash")
    print(f'   export OPENAI_BASE_URL="{CONFIG["api_url"]}"')
    print(f'   export OPENAI_API_KEY="{CONFIG["api_key"]}"')
    print(f'   export DEFAULT_MODEL="{CONFIG["model"]}"')
    print("   ```")
    
    print("\n4. **可能的兼容性问题:**")
    print("   - 需要API支持OpenAI兼容格式")
    print("   - 可能需要流式响应支持")
    print("   - 上下文长度可能有限制")
    print("   - 可能需要特定的端点路径")

def generate_config_examples():
    """生成配置示例"""
    print_header("配置示例文件")
    
    print("📁 **Python配置示例** (config.py)")
    print("```python")
    print("# LLM API配置")
    print(f'API_BASE_URL = "{CONFIG["api_url"]}"')
    print(f'API_KEY = "{CONFIG["api_key"]}"')
    print(f'MODEL_NAME = "{CONFIG["model"]}"')
    print(f'MAX_TOKENS = {CONFIG["max_tokens"]}')
    print()
    print("# OpenAI兼容客户端")
    print('def create_llm_client():')
    print('    from openai import OpenAI')
    print(f'    return OpenAI(')
    print(f'        base_url=API_BASE_URL,')
    print(f'        api_key=API_KEY')
    print('    )')
    print("```")
    
    print("\n📁 **环境变量文件** (.env)")
    print("```env")
    print("# LLM API配置")
    print(f'OPENAI_BASE_URL={CONFIG["api_url"]}')
    print(f'OPENAI_API_KEY={CONFIG["api_key"]}')
    print(f'LLM_MODEL={CONFIG["model"]}')
    print(f'MAX_CONTEXT_LENGTH={CONFIG["max_tokens"]}')
    print("```")
    
    print("\n📁 **快速测试脚本** (test_connection.py)")
    print("```python")
    print('import requests')
    print('import json')
    print()
    print(f'url = "{CONFIG["api_url"]}"')
    print(f'headers = {{"Authorization": "Bearer {CONFIG["api_key"]}"}}')
    print('data = {')
    print(f'    "model": "{CONFIG["model"]}",')
    print('    "messages": [')
    print('        {"role": "user", "content": "Hello, are you working?"}')
    print('    ],')
    print('    "max_tokens": 50')
    print('}')
    print()
    print('response = requests.post(url, headers=headers, json=data)')
    print('print(f"Status: {response.status_code}")')
    print('print(f"Response: {response.text}")')
    print("```")

def main():
    print("🔬 全面LLM API测试工具")
    print(f"📊 配置信息:")
    print(f"   端  点: {CONFIG['api_url']}")
    print(f"   模  型: {CONFIG['model']}")
    print(f"   API Key: {CONFIG['api_key'][:12]}...{CONFIG['api_key'][-4:]}")
    print(f"   窗  口: {CONFIG['max_tokens']:,} tokens")
    print(f"{'='*80}")
    
    # 执行测试
    test_api_connection()
    test_format_openai()
    test_format_alternative()
    test_model_capabilities()
    test_claude_code_integration()
    generate_config_examples()
    
    print(f"\n{'='*80}")
    print("📋 测试完成总结")
    print(f"{'='*80}")
    
    print("\n💡 **关键发现:**")
    print("1. 如果API端点响应200，说明服务正常")
    print("2. 需要正确的请求格式才能获得模型响应")
    print("3. 确认是否能返回JSON格式（而不是HTML）")
    print("4. 确认回复中是否包含正确的模型输出")
    
    print("\n🔧 **Claude Code集成建议:**")
    print("1. 确认API完全兼容OpenAI格式")
    print("2. 测试流式响应是否正常工作")
    print(f"3. 验证上下文窗口大小: {CONFIG['max_tokens']} tokens")
    print("4. 检查推理能力和代码生成质量")
    
    print("\n📁 **下一步:**")
    print("1. 如果所有测试失败，可能需要联系API提供商")
    print("2. 请求更详细的API文档")
    print("3. 确认是否需要特定的授权方式")
    print("4. 尝试使用提供的配置示例进行集成")

if __name__ == "__main__":
    main()