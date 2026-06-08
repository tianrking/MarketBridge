const els = {
  apiBase: document.querySelector("#apiBase"),
  exchange: document.querySelector("#exchange"),
  quote: document.querySelector("#quote"),
  threshold: document.querySelector("#threshold"),
  limit: document.querySelector("#limit"),
  refreshBtn: document.querySelector("#refreshBtn"),
  exportBtn: document.querySelector("#exportBtn"),
  status: document.querySelector("#status"),
  matchCount: document.querySelector("#matchCount"),
  lowestRate: document.querySelector("#lowestRate"),
  updatedAt: document.querySelector("#updatedAt"),
  rows: document.querySelector("#rows"),
};

let currentRows = [];

function defaultApiBase() {
  const saved = localStorage.getItem("marketbridge.apiBase");
  if (saved) return saved;
  const params = new URLSearchParams(window.location.search);
  const explicit = params.get("api");
  if (explicit) return explicit;
  if (["localhost", "127.0.0.1", ""].includes(window.location.hostname)) {
    return "http://127.0.0.1:8080";
  }
  return `${window.location.origin}/api`;
}

function setStatus(text, tone = "idle") {
  els.status.textContent = text;
  els.status.style.color = tone === "error" ? "#b42318" : tone === "ok" ? "#0f766e" : "";
}

function formatNumber(value, digits = 6) {
  const num = Number(value);
  if (!Number.isFinite(num)) return "-";
  return num.toLocaleString("en-US", {
    maximumFractionDigits: digits,
  });
}

function formatTime(ms) {
  const num = Number(ms);
  if (!Number.isFinite(num) || num <= 0) return "-";
  return new Date(num).toLocaleString("zh-CN", {
    year: "numeric",
    month: "2-digit",
    day: "2-digit",
    hour: "2-digit",
    minute: "2-digit",
    hour12: false,
  });
}

function buildUrl() {
  const base = els.apiBase.value.replace(/\/+$/, "");
  const url = new URL(`${base}/v1/market/perpetual-funding`);
  url.searchParams.set("exchange", els.exchange.value);
  url.searchParams.set("quote", els.quote.value.trim().toUpperCase() || "USDT");
  url.searchParams.set("limit", String(Math.max(1, Number(els.limit.value) || 50000)));
  return url;
}

function render(rows) {
  currentRows = rows;
  els.matchCount.textContent = String(rows.length);
  els.lowestRate.textContent = rows.length ? `${formatNumber(rows[0].funding_rate_pct, 6)}%` : "-";
  els.updatedAt.textContent = new Date().toLocaleTimeString("zh-CN", {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
    hour12: false,
  });

  if (!rows.length) {
    els.rows.innerHTML = '<tr><td colspan="5" class="empty">No matching contracts</td></tr>';
    return;
  }

  els.rows.innerHTML = rows
    .map(
      (row) => `
        <tr>
          <td>${escapeHtml(row.symbol)}</td>
          <td>${escapeHtml(row.exchange)}</td>
          <td class="num rate-negative">${formatNumber(row.funding_rate_pct, 6)}%</td>
          <td class="num">${formatNumber(row.mark_price, 8)}</td>
          <td>${formatTime(row.next_funding_time_ms)}</td>
        </tr>
      `,
    )
    .join("");
}

function escapeHtml(value) {
  return String(value ?? "")
    .replaceAll("&", "&amp;")
    .replaceAll("<", "&lt;")
    .replaceAll(">", "&gt;")
    .replaceAll('"', "&quot;")
    .replaceAll("'", "&#039;");
}

async function refresh() {
  els.refreshBtn.disabled = true;
  setStatus("Loading...");
  try {
    localStorage.setItem("marketbridge.apiBase", els.apiBase.value.trim());
    const threshold = Number(els.threshold.value);
    const res = await fetch(buildUrl(), { headers: { Accept: "application/json" } });
    if (!res.ok) {
      throw new Error(`HTTP ${res.status}`);
    }
    const data = await res.json();
    const rows = Array.isArray(data.funding) ? data.funding : [];
    const filtered = rows
      .filter((row) => Number(row.funding_rate_pct) < threshold)
      .sort((a, b) => Number(a.funding_rate_pct) - Number(b.funding_rate_pct));
    render(filtered);
    setStatus(`OK: ${rows.length} rows`, "ok");
  } catch (error) {
    currentRows = [];
    render([]);
    setStatus(error.message || "Request failed", "error");
  } finally {
    els.refreshBtn.disabled = false;
  }
}

function exportCsv() {
  const header = ["exchange", "symbol", "funding_rate_pct", "mark_price", "next_funding_time_ms"];
  const lines = [
    header.join(","),
    ...currentRows.map((row) =>
      header.map((key) => JSON.stringify(row[key] ?? "")).join(","),
    ),
  ];
  const blob = new Blob([`${lines.join("\n")}\n`], { type: "text/csv;charset=utf-8" });
  const url = URL.createObjectURL(blob);
  const link = document.createElement("a");
  link.href = url;
  link.download = `${els.exchange.value}-negative-funding.csv`;
  link.click();
  URL.revokeObjectURL(url);
}

els.apiBase.value = defaultApiBase();
els.refreshBtn.addEventListener("click", refresh);
els.exportBtn.addEventListener("click", exportCsv);
for (const input of [els.exchange, els.quote, els.threshold, els.limit]) {
  input.addEventListener("change", refresh);
}

refresh();
