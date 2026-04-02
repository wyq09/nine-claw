#!/usr/bin/env python3
"""
寻找正确的API端点
"""

import requests
import json

def test_endpoint_patterns():
    """测试可能的端点模式"""
    
    base_url = "https://chenyu.pro"
    api_key = "***REMOVED***06b6479e"
    
    headers = {"Authorization": f"Bearer {api_key}"}
    
    # 可能的端点模式
    patterns = [
        # OpenAI兼容格式
        "/v1/chat/completions",
        "/api/v1/chat/completions", 
        "/chat/completions",
        "/api/chat/completions",
        
        # 通用API格式
        "/api/completion",
        "/api/v1/completion",
        "/api/generate",
        "/api/v1/generate",
        
        # 简洁格式
        "/api/complete",
        "/api/v1/complete",
        "/complete",
        
        # 模型特定
        "/api/llm",
        "/llm",
        "/api/model",
        "/model",
        
        # 可能有版本号
        "/v1/completions",
        "/v1/chat",
        "/api/chat",
        
        # 其他常见格式
        "/api/predict",
        "/predict",
        "/api/infer",
        "/infer",
        
        # 尝试根路径下的API
        "/api",
        "/v1",
        
        # 尝试websocket
        "/api/v1/chat/completions/stream",
        "/api/stream",
        "/stream"
    ]
    
    print("🔍 测试可能的API端点模式...")
    print(f"📡 Base URL: {base_url}")
    print(f"🔑 API Key: {api_key[:12]}...{api_key[-4:]}")
    print("="*80)
    
    results = []
    
    for pattern in patterns:
        url = base_url + pattern
        
        # 首先测试GET
        try:
            get_response = requests.get(url, headers=headers, timeout=5)
            get_status = get_response.status_code
            get_content_type = get_response.headers.get('content-type', '')
        except Exception as e:
            get_status = f"GET Error: {e}"
            get_content_type = ""
        
        # 然后测试POST（带简单数据）
        try:
            post_data = {"test": "hello"}
            post_response = requests.post(url, headers=headers, json=post_data, timeout=5)
            post_status = post_response.status_code
            post_content_type = post_response.headers.get('content-type', '')
            post_body = post_response.text[:100]
        except Exception as e:
            post_status = f"POST Error: {e}"
            post_content_type = ""
            post_body = ""
        
        results.append({
            "pattern": pattern,
            "get_status": get_status,
            "get_content_type": get_content_type,
            "post_status": post_status,
            "post_content_type": post_content_type,
            "post_body": post_body
        })
        
        # 显示有趣的结果
        interesting = False
        if isinstance(get_status, int) and get_status != 404 and get_status != 405:
            interesting = True
        if isinstance(post_status, int) and post_status != 404 and post_status != 405:
            interesting = True
        if "json" in get_content_type or "json" in post_content_type:
            interesting = True
            
        if interesting:
            print(f"\n🔎 {pattern}:")
            if isinstance(get_status, int):
                print(f"   GET:  {get_status} ({get_content_type})")
            else:
                print(f"   GET:  {get_status}")
                
            if isinstance(post_status, int):
                print(f"   POST: {post_status} ({post_content_type})")
                if post_body:
                    print(f"        {post_body}")
            else:
                print(f"   POST: {post_status}")
    
    # 保存所有结果
    with open("endpoint_pattern_results.json", "w", encoding="utf-8") as f:
        json.dump(results, f, ensure_ascii=False, indent=2)
    
    print("\n" + "="*80)
    print("📊 分析结果:")
    
    # 统计
    get_200 = sum(1 for r in results if r["get_status"] == 200)
    post_200 = sum(1 for r in results if r["post_status"] == 200)
    json_responses = sum(1 for r in results if "json" in r["get_content_type"] or "json" in r["post_content_type"])
    
    print(f"   GET 200 响应: {get_200}")
    print(f"   POST 200 响应: {post_200}")
    print(f"   JSON 响应: {json_responses}")
    
    # 显示所有返回200的端点
    print(f"\n✅ 成功响应的端点:")
    for r in results:
        if r["get_status"] == 200 or r["post_status"] == 200:
            status = f"GET:{r['get_status']}/POST:{r['post_status']}"
            content_type = r["post_content_type"] or r["get_content_type"]
            print(f"   {r['pattern']} - {status} - {content_type}")

def check_api_documentation():
    """检查可能的API文档位置"""
    print("\n🔍 查找API文档...")
    
    base_url = "https://chenyu.pro"
    common_doc_paths = [
        "/docs",
        "/documentation", 
        "/api-docs",
        "/api/documentation",
        "/swagger",
        "/swagger-ui",
        "/openapi",
        "/redoc",
        "/api",
        "/v1/api-docs",
        "/help",
        "/guide",
        "/api-guide",
        "/api/v1/docs"
    ]
    
    for path in common_doc_paths:
        url = base_url + path
        try:
            response = requests.get(url, timeout=5)
            if response.status_code == 200:
                print(f"📄 {path}: {response.status_code}")
                # 检查是否是HTML页面
                content_type = response.headers.get('content-type', '')
                if "html" in content_type:
                    # 尝试查找标题
                    html = response.text.lower()
                    if "api" in html or "documentation" in html or "swagger" in html:
                        print(f"   🔍 可能包含API文档")
                elif "json" in content_type:
                    print(f"   📊 可能是OpenAPI/Swagger文档")
        except:
            pass

def main():
    print("🔬 寻找正确的LLM API端点")
    print("="*80)
    
    test_endpoint_patterns()
    check_api_documentation()
    
    print("\n" + "="*80)
    print("💡 分析和建议:")
    print("\n1. **如果所有测试都失败**:")
    print("   - API端点可能已更改")
    print("   - 可能需要使用WebSocket连接")
    print("   - 服务可能已停止或需要特定配置")
    
    print("\n2. **如果看到401错误**:")
    print("   - API Key可能是有效的")
    print("   - 但需要正确的端点路径")
    
    print("\n3. **下一步行动建议**:")
    print("   a. 联系服务提供商获取最新API文档")
    print("   b. 检查是否有GitHub仓库或示例代码")
    print("   c. 尝试其他兼容的LLM API")
    print("   d. 确认是否需要WebSocket连接")
    
    print(f"\n📁 详细结果已保存到: endpoint_pattern_results.json")

if __name__ == "__main__":
    main()