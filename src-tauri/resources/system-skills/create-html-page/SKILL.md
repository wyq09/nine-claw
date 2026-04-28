---
name: create-html-page
description: Create and publish HTML pages on YouYong AI Charts (aichart.youyongai.com). Use when user wants to create, deploy, or publish an HTML page to a shareable URL.
triggers:
  - "create html page"
  - "publish html"
  - "deploy html page"
  - "create a webpage"
  - "make an html page"
  - "upload html"
---

# Create HTML Page on YouYong AI

You can create and publish HTML pages that are instantly accessible via a unique URL.

## Prerequisites

The user needs an **API token** from [YouYong AI](https://aichart.youyongai.com/account/api-tokens).

If the user doesn't have a token, direct them to:
1. Register/login at https://aichart.youyongai.com/login
2. Go to https://aichart.youyongai.com/account/api-tokens
3. Create a new API token and copy it

## Configuration

Before first use, set the API token as an environment variable:

```bash
export YOUYONG_API_TOKEN="sk_your_token_here"
```

Or add it to your `.env` file:
```
YOUYONG_API_TOKEN=sk_your_token_here
```

## Usage

### Basic Page Creation

When the user asks you to create an HTML page:

1. **Generate the HTML content** based on the user's requirements
2. **Call the API** using the Bash tool:

```bash
curl -s -X POST https://aichart.youyongai.com/api/html-pages \
  -H "Authorization: Bearer $YOUYONG_API_TOKEN" \
  -H "Content-Type: application/json" \
  -d "$(cat <<'EOF'
{
  "page_name": "Page Title",
  "html_content": "YOUR_COMPLETE_HTML_HERE",
  "custom_path": "my-custom-path",
  "require_password": false
}
EOF
)"
```

### Parameters

| Parameter | Required | Description |
|-----------|----------|-------------|
| `page_name` | No | Display name for the page (max 255 chars) |
| `html_content` | Yes | Full HTML content (max 10MB) |
| `custom_path` | No | Custom URL path. Page accessible at `/html-pages/{custom_path}` (max 60 chars, alphanumeric + hyphens/underscores only) |
| `require_password` | No | Set `true` to require a password to view (default: false) |
| `access_password` | No | Custom password (max 15 chars). Auto-generated if not provided when `require_password` is true |

### Response

On success, the API returns:
```json
{
  "success": true,
  "en_code": "abc123...",
  "url": "/html-pages/abc123",
  "full_url": "https://aichart.youyongai.com/html-pages/abc123",
  "message": "HTML page created successfully"
}
```

Always present the `full_url` to the user as the link to access their page.

### Error Handling

- `401`: Invalid or missing API token
- `409 DUPLICATE_CUSTOM_PATH`: Custom path already taken — suggest a different one
- `409 DUPLICATE_HTML`: Same HTML content already exists — return existing URL
- `400 VALIDATION_ERROR`: Check parameter constraints

## Workflow

When the user asks to create an HTML page:

1. Clarify requirements: page content, custom path preference, password protection
2. Write the complete HTML (inline CSS, self-contained, responsive)
3. Escape the HTML content properly for JSON
4. Call the API
5. Return the **full_url** to the user

## Tips

- Create **self-contained HTML** with inline CSS and embedded JS — no external dependencies
- Use **responsive design** so pages work on mobile
- For complex pages, test the HTML locally first if possible
- The `custom_path` must be unique across all users — suggest descriptive paths
- If the user doesn't specify a custom path, omit it and the system auto-generates one
