import { fetchUrlWithCurl } from "./curl_http.mjs";

export async function fetchSearchResponse(searchUrl, language, deps, signal, tempArtifactTracker) {
  const headers = {
    accept: "text/html,application/xhtml+xml,application/xml;q=0.9,image/avif,image/webp,*/*;q=0.8",
    "accept-language": language ?? "zh-CN,zh;q=0.9,en;q=0.8",
    "cache-control": "no-cache",
    "user-agent":
      "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/126.0 Safari/537.36",
  };

  if (typeof deps.execFileImpl === "function") {
    const curlResponse = await fetchUrlWithCurl(
      searchUrl,
      {
        userAgent: headers["user-agent"],
        headers: Object.entries(headers)
          .filter(([key]) => key !== "user-agent")
          .map(([key, value]) => `${key}: ${value}`),
      },
      deps,
      tempArtifactTracker,
    );
    return {
      status: curlResponse.status,
      ok: curlResponse.status >= 200 && curlResponse.status < 400,
      searchUrl,
      finalUrl: curlResponse.finalUrl,
      html: curlResponse.bodyBuffer.toString("utf8"),
      transport: curlResponse.transport,
    };
  }

  const response = await deps.fetchImpl(searchUrl, {
    method: "GET",
    signal,
    headers,
  });
  return {
    status: response.status,
    ok: response.ok,
    searchUrl,
    finalUrl: searchUrl,
    html: await response.text(),
    transport: "fetch",
  };
}

export function detectSearchInterstitial(engine, html, results) {
  if (results.length > 0) {
    return undefined;
  }
  const normalized = String(html ?? "");
  if (
    (engine.key === "google" || engine.key === "google_hk") &&
    /httpservice\/retry\/enablejs|<title>\s*Google Search\s*<\/title>/i.test(normalized)
  ) {
    return "Google returned an anti-bot / enablejs interstitial instead of search results.";
  }
  if (/captcha|verify you are human|unusual traffic|robot/i.test(normalized)) {
    return `${engine.name} returned an anti-bot challenge page.`;
  }
  return undefined;
}
