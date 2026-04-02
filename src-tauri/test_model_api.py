#!/usr/bin/env python3
"""
测试LLM模型API连接
端口： https://chenyu.pro/api/v1/llm
模型：Jackrong/Qwen3.5-27B-Claude-4.6-Opus-Reasoning-Distilled-v2
key：***REMOVED***06b6479e
"""

import requests
import json
import time
from typing import Dict, Any

def test_model_api():
    """测试模型API连接"""
    
    # API配置
    api_url = "https://chenyu.pro/api/v1/llm"
    
    # 您的配置参数
    config = {
        "model": "Jackrong/Qwen3.5-27B-Claude-4.6-Opus-Reasoning-Distilled-v2",
        "api_key": "***REMOVED***06b6479e",
        "max_tokens": 256000,  # 窗口大小
        "temperature": 0.7,
        "timeout": 30
    }
    
    print("🔧 开始测试模型API连接...")
    print("=" * 60)
    print(f"API端点: {api_url}")
    print(f"模型名称: {config['model']}")
    print(f"API Key: {config['api_key'][:12]}...{config['api_key'][-4:]}")
    print(f"窗口大小: {config['max_tokens']:,} tokens")
    print("=" * 60)
    
    # 构建请求头
    headers = {
        "Authorization": f"Bearer {config['api_key']}",
        "Content-Type": "application/json",
        "Accept": "application/json"
    }
    
    # 简单的测试消息
    test_messages = [
        {
            "role": "user",
            "content": "你好，请简单介绍一下自己。如果看到这条消息，请回复'API测试成功！我能正常运行。'"
        }
    ]
    
    # 构建请求体
    payload = {
        "model": config["model"],
        "messages": test_messages,
        "max_tokens": 500,  # 限制回复长度
        "temperature": config["temperature"],
        "stream": False
    }
    
    test_results = {
        "api_url": api_url,
        "model": config["model"],
        "status": "unknown",
        "response_time": 0,
        "error": None,
        "raw_response": None,
        "test_message": test_messages[0]["content"]
    }
    
    try:
        print("📡 发送测试请求...")
        start_time = time.time()
        
        # 发送请求
        response = requests.post(
            api_url,
            headers=headers,
            json=payload,
            timeout=config["timeout"]
        )
        
        response_time = time.time() - start_time
        test_results["response_time"] = response_time
        
        print(f"⏱️  响应时间: {response_time:.2f}秒")
        print(f"📨 状态码: {response.status_code}")
        
        # 尝试解析响应
        if response.status_code == 200:
            try:
                data = response.json()
                test_results["raw_response"] = data
                
                print("✅ API连接成功！")
                print("📋 响应格式检查:")
                print(f"   - 响应类型: {type(data)}")
                
                # 检查常见的响应字段
                if isinstance(data, dict):
                    keys = list(data.keys())
                    print(f"   - 响应字段: {keys}")
                    
                    # 提取可能的回复内容
                    if 'choices' in data and data['choices']:
                        if 'message' in data['choices'][0]:
                            content = data['choices'][0]['message'].get('content', '无内容')
                            print(f"   - AI回复: {content[:100]}...")
                        elif 'text' in data['choices'][0]:
                            content = data['choices'][0].get('text', '无内容')
                            print(f"   - AI回复: {content[:100]}...")
                    elif 'response' in data:
                        content = data['response']
                        print(f"   - AI回复: {content[:100]}...")
                    elif 'content' in data:
                        content = data['content']
                        print(f"   - AI回复: {content[:100]}...")
                    else:
                        print(f"   - 完整响应: {json.dumps(data, ensure_ascii=False)[:200]}...")
                
                test_results["status"] = "success"
                
            except json.JSONDecodeError as e:
                print(f"❌ JSON解析失败: {e}")
                print(f"原始响应文本: {response.text[:200]}...")
                test_results["status"] = "json_error"
                test_results["error"] = f"JSON解析失败: {str(e)}"
                test_results["raw_response"] = response.text
                
        else:
            print(f"❌ API请求失败，状态码: {response.status_code}")
            print(f"错误响应: {response.text[:200]}...")
            test_results["status"] = f"http_error_{response.status_code}"
            test_results["error"] = f"HTTP {response.status_code}: {response.text[:200]}"
            test_results["raw_response"] = response.text
            
    except requests.exceptions.Timeout:
        print("⏰ 请求超时！")
        test_results["status"] = "timeout"
        test_results["error"] = f"请求超过{config['timeout']}秒"
        
    except requests.exceptions.ConnectionError as e:
        print(f"🔌 连接失败: {e}")
        test_results["status"] = "connection_error"
        test_results["error"] = str(e)
        
    except requests.exceptions.RequestException as e:
        print(f"⚠️  请求异常: {e}")
        test_results["status"] = "request_error"
        test_results["error"] = str(e)
        
    except Exception as e:
        print(f"💥 未知错误: {type(e).__name__}: {e}")
        test_results["status"] = "unknown_error"
        test_results["error"] = f"{type(e).__name__}: {str(e)}"
    
    print("=" * 60)
    print("🧪 执行更多测试...")
    
    # 测试2: 简单的模型列表查询
    print("\n📋 测试2: 查询模型列表...")
    try:
        models_payload = {
            "model": config["model"],
            "messages": [{"role": "user", "content": "列出你可用的模型"}],
            "max_tokens": 200
        }
        
        models_response = requests.post(
            api_url,
            headers=headers,
            json=models_payload,
            timeout=15
        )
        
        if models_response.status_code == 200:
            print("✅ 模型列表查询成功")
            models_data = models_response.json()
            print(f"   - 响应类型: {type(models_data)}")
        else:
            print(f"❌ 模型列表查询失败: {models_response.status_code}")
            
    except Exception as e:
        print(f"❌ 模型列表查询异常: {e}")
    
    # 测试3: 复杂的推理测试
    print("\n🤔 测试3: 简单推理测试...")
    try:
        reasoning_payload = {
            "model": config["model"],
            "messages": [{
                "role": "user", 
                "content": "如果A比B大3岁，B比C小5岁，那么A比C大多少岁？请一步步推理。"
            }],
            "max_tokens": 500,
            "temperature": 0.3
        }
        
        reasoning_response = requests.post(
            api_url,
            headers=headers,
            json=reasoning_payload,
            timeout=20
        )
        
        if reasoning_response.status_code == 200:
            print("✅ 推理测试成功")
            reasoning_data = reasoning_response.json()
            # 提取回复
            if isinstance(reasoning_data, dict):
                if 'choices' in reasoning_data and reasoning_data['choices']:
                    choice = reasoning_data['choices'][0]
                    if 'message' in choice:
                        reply = choice['message'].get('content', '无内容')
                        print(f"   - AI推理: {reply[:150]}...")
        else:
            print(f"❌ 推理测试失败: {reasoning_response.status_code}")
            
    except Exception as e:
        print(f"❌ 推理测试异常: {e}")
    
    # 保存测试结果
    print("\n💾 保存测试结果...")
    with open("model_api_test_result.json", "w", encoding="utf-8") as f:
        json.dump(test_results, f, ensure_ascii=False, indent=2)
    
    print("=" * 60)
    
    # 最终总结
    print("📊 测试结果总结:")
    print(f"   API状态: {test_results['status']}")
    print(f"   响应时间: {test_results['response_time']:.2f}秒")
    
    if test_results['status'] == 'success':
        print("   ✅ 模型API可以正常使用！")
        print("   💡 建议: 可以进行实际应用测试")
    else:
        print(f"   ❌ 模型API存在问题: {test_results.get('error', '未知错误')}")
        print("   💡 建议: 检查API密钥、网络连接或模型名称")
    
    print(f"   详细结果已保存到: model_api_test_result.json")
    
    return test_results

if __name__ == "__main__":
    print("🔍 LLM模型API测试工具")
    print("=" * 60)
    
    try:
        results = test_model_api()
        
        # 简单的健康状况判断
        if results["status"] == "success":
            print("\n🎉 测试完成！模型API运行正常！")
            print("您可以开始使用这个模型进行开发和测试。")
        else:
            print("\n⚠️  测试完成！但发现一些问题。")
            print("请检查API配置或联系服务提供商。")
            
    except KeyboardInterrupt:
        print("\n🛑 用户中断测试")
    except Exception as e:
        print(f"\n💥 测试过程中出现严重错误: {e}")
        import traceback
        traceback.print_exc()