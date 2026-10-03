"use strict";

// The launch secret arrives in the URL fragment, which is never sent to a server. Keep it for this tab only
// and take it out of the address bar and history.
const secret = (() => {
  const fragment = location.hash.slice(1);
  if (fragment) {
    sessionStorage.setItem("ws-secret", fragment);
    history.replaceState(null, "", location.pathname);
  }
  return sessionStorage.getItem("ws-secret");
})();

const POLL_MS = 5000;
const state = { mode: "read", accounts: [], selected: null, stopped: false };

async function api(path, method = "GET") {
  const response = await fetch(`/api${path}`, { method, headers: { "x-ws-secret": secret ?? "" }, cache: "no-store" });
  const body = await response.json().catch(() => null);
  if (response.status === 401) gate("This page belongs to an earlier launch. Run `ws-mcp dashboard` again.");
  if (!response.ok) throw new Error(body?.error ?? `${response.status} ${response.statusText}`);
  return body;
}

// Every value goes in as text, never as markup.
function el(tag, props = {}, ...children) {
  const node = document.createElement(tag);
  for (const [key, value] of Object.entries(props)) {
    if (key === "class") node.className = value;
    else if (key === "onclick") node.addEventListener("click", value);
    else node[key] = value;
  }
  for (const child of children.flat()) {
    if (child !== null && child !== undefined && child !== false) node.append(child instanceof Node ? child : String(child));
  }
  return node;
}

function show(id, ...children) {
  document.getElementById(id).replaceChildren(...children);
}

function table(columns, rows) {
  const head = el("tr", {}, columns.map((c) => el("th", { class: c.number ? "number" : "" }, c.title)));
  const body = rows.map((row) => el("tr", {}, row.map((cell, i) => el("td", { class: columns[i].number ? "number" : "" }, cell))));
  return el("table", {}, el("thead", {}, head), el("tbody", {}, body));
}

function money(m) {
  if (!m) return "-";
  try {
    return new Intl.NumberFormat(undefined, { style: "currency", currency: m.currency }).format(Number(m.amount));
  } catch {
    return `${m.amount} ${m.currency}`;
  }
}

function ago(iso) {
  if (!iso) return "-";
  const seconds = (Date.now() - new Date(iso).getTime()) / 1000;
  if (seconds < 60) return "just now";
  if (seconds < 3600) return `${Math.floor(seconds / 60)} min ago`;
  if (seconds < 86400) return `${Math.floor(seconds / 3600)} h ago`;
  return new Date(iso).toLocaleDateString();
}

function until(iso) {
  const minutes = Math.max(0, Math.round((new Date(iso).getTime() - Date.now()) / 60000));
  return minutes < 1 ? "expires in under a minute" : `expires in ${minutes} min`;
}

function words(snake) {
  const text = String(snake ?? "").replaceAll("_", " ");
  return text.charAt(0).toUpperCase() + text.slice(1);
}

function accountName(account) {
  return account.nickname ?? kind(account.registration);
}

function kind(registration) {
  const names = { tfsa: "TFSA", rrsp: "RRSP", spousal_rrsp: "Spousal RRSP", fhsa: "FHSA", resp: "RESP", rrif: "RRIF", lira: "LIRA" };
  return typeof registration === "string" ? names[registration] ?? words(registration) : words(Object.values(registration ?? {})[0]);
}

function accountById(id) {
  const account = state.accounts.find((a) => a.id === id);
  return account ? accountName(account) : id;
}

function describe(order, symbol) {
  if (!order) return "";
  const size = order.size.shares !== undefined
    ? `${order.size.shares} ${order.size.shares === "1" ? "share" : "shares"}`
    : money(order.size.value);
  const price = order.kind.type === "market"
    ? "at market"
    : order.kind.type === "limit"
      ? `limit ${money(order.kind.limit)}`
      : `stop ${money(order.kind.stop)}, limit ${money(order.kind.limit)}`;
  return `${words(order.side)} ${size}${symbol ? ` of ${symbol}` : ""}, ${price}${order.tif === "gtc" ? ", until cancelled" : ""}`;
}

let toastTimer = null;
function toast(message, error = false) {
  const node = document.getElementById("toast");
  node.textContent = message;
  node.className = error ? "toast error" : "toast";
  node.hidden = false;
  clearTimeout(toastTimer);
  toastTimer = setTimeout(() => { node.hidden = true; }, error ? 8000 : 3000);
}

function gate(message) {
  state.stopped = true;
  document.querySelector(".bar").hidden = true;
  document.getElementById("main").hidden = true;
  document.getElementById("gate").hidden = false;
  document.getElementById("gate-message").textContent = message;
}

// --- sections ---

function stat(label, value, sub, used, max) {
  const card = el("div", { class: "stat" }, el("div", { class: "label" }, label), el("div", { class: "value" }, value));
  if (sub) card.append(el("div", { class: "sub" }, sub));
  if (max !== undefined) {
    const fill = el("span");
    const ratio = max > 0 ? Math.min(1, used / max) : 0;
    fill.style.width = `${ratio * 100}%`;
    card.append(el("div", { class: ratio >= 1 ? "meter full" : "meter" }, fill));
  }
  return card;
}

async function overview() {
  const status = await api("/overview");
  state.mode = status.mode;
  const mode = document.getElementById("mode");
  mode.textContent = status.mode;
  mode.className = `pill ${status.mode}`;
  const indicator = document.getElementById("state");
  indicator.textContent = status.killed ? "Trading stopped" : status.mode === "read" ? "Read only" : "Trading allowed";
  indicator.className = status.killed ? "state stopped" : "state";
  document.getElementById("kill").hidden = status.killed;
  document.getElementById("resume").hidden = !status.killed;

  if (status.orders_today === undefined) {
    show("stats", stat("Read mode", "No orders", "The agent can look but not trade. Run `ws-mcp setup` to change that."));
    show("rules");
    return;
  }
  const spent = Number(status.spent_today.amount);
  const budget = Number(status.max_daily_spend.amount);
  show(
    "stats",
    stat("Orders today", `${status.orders_today} of ${status.max_orders_per_day}`, null, status.orders_today, status.max_orders_per_day),
    stat("Spent today", money(status.spent_today), `of ${money(status.max_daily_spend)}`, spent, budget),
    stat("Largest order", money(status.max_order_value), "per order"),
    stat("Needs approval", `over ${money(status.require_approval_above)}`, "approve here or with `ws-mcp approve`"),
  );
  const rules = [
    status.limit_only && "Limit orders only",
    `Limit prices at most ${status.max_limit_deviation_pct}% worse than market`,
    status.market_hours_only && "Market hours only",
  ].filter(Boolean);
  show("rules", ...rules.map((rule) => el("span", { class: "rule" }, rule)));
}

async function approvals() {
  const tickets = await api("/approvals");
  document.getElementById("approvals-panel").hidden = tickets.length === 0;
  show("approvals", ...tickets.map((ticket) => el("div", { class: "ticket" },
    el("div", { class: "what" },
      el("div", { class: "title" }, describe(ticket.order, ticket.symbol ?? ticket.order.security)),
      el("div", { class: "secondary" }, `In ${accountById(ticket.order.account)} · about ${money(ticket.value)} · ${until(ticket.expires)}`),
    ),
    el("button", {
      class: "primary",
      onclick: () => act(
        `Approve this order?\n\n${describe(ticket.order, ticket.symbol ?? ticket.order.security)}\nAbout ${money(ticket.value)}`,
        `/approvals/${encodeURIComponent(ticket.id)}`,
        "Approved. The agent can place it now.",
      ),
    }, "Approve"),
  )));
}

async function accounts() {
  state.accounts = await api("/accounts");
  renderAccounts();
}

function renderAccounts() {
  if (state.accounts.length === 0) {
    show("accounts", el("p", { class: "muted" }, "No accounts."));
    return;
  }
  show("accounts", ...state.accounts.map((account) => el("button", {
    type: "button",
    class: account.id === state.selected ? "account selected" : "account",
    onclick: () => openAccount(account),
  },
    el("span", { class: "name" }, accountName(account)),
    el("span", { class: "worth" }, money(account.valuation?.value)),
    el("span", { class: "meta" }, `${kind(account.registration)} · ${words(account.management)} · ${account.currency}`),
  )));
}

async function openAccount(account) {
  state.selected = account.id;
  renderAccounts();
  const heading = el("div", { class: "detail-head" },
    el("div", {}, el("h2", {}, accountName(account)), el("p", { class: "muted" }, `${kind(account.registration)} · ${words(account.management)}`)),
    el("span", { class: "worth" }, money(account.valuation?.value)),
  );
  show("detail", heading, el("p", { class: "muted placeholder" }, "Loading…"));

  const id = encodeURIComponent(account.id);
  let positions, orders;
  try {
    [positions, orders] = await Promise.all([api(`/accounts/${id}/positions`), api(`/accounts/${id}/orders`)]);
  } catch (error) {
    show("detail", heading, el("p", { class: "inline-error placeholder" }, String(error.message ?? error)));
    return;
  }
  if (state.selected !== account.id) return;

  const holdings = positions.length === 0
    ? el("p", { class: "muted" }, "No holdings.")
    : table(
      [{ title: "Security" }, { title: "Shares", number: true }, { title: "Value", number: true }, { title: "Cost", number: true }],
      positions.map((p) => [
        el("div", {}, el("div", { class: "symbol" }, p.security.symbol), el("div", { class: "secondary" }, p.security.name)),
        p.quantity, money(p.value), money(p.cost),
      ]),
    );
  const open = orders.length === 0
    ? el("p", { class: "muted" }, "No open orders.")
    : table(
      [{ title: "Order" }, { title: "Status" }, { title: "Placed" }, { title: "" }],
      orders.map((o) => [
        el("div", {},
          el("div", { class: "symbol" }, `${words(o.side)} ${o.quantity ?? ""} ${o.symbol ?? ""}`.trim()),
          el("div", { class: "secondary" }, o.limit ? `limit ${money(o.limit)}` : "at market"),
        ),
        words(o.status),
        ago(o.created),
        o.key && state.mode !== "read"
          ? el("button", {
            class: "small outline-danger",
            onclick: () => act(
              `Cancel this order?\n\n${words(o.side)} ${o.quantity ?? ""} ${o.symbol ?? o.security}`,
              `/orders/${encodeURIComponent(o.key)}/cancel`,
              "Order cancelled.",
            ),
          }, "Cancel")
          : "",
      ]),
    );
  show("detail", heading, el("h3", {}, "Holdings"), holdings, el("h3", {}, "Open orders"), open);
}

async function audit() {
  const entries = await api("/audit?limit=50");
  if (entries.length === 0) {
    show("audit", el("p", { class: "muted" }, "Nothing yet. Previews, orders, approvals, and cancels show up here."));
    return;
  }
  show("audit", el("div", { class: "feed" }, entries.map((e) => el("div", { class: "event" },
    el("span", { class: "when" }, ago(e.at)),
    el("div", {},
      el("div", {}, [words(e.action), describe(e.order)].filter(Boolean).join(": ")),
      e.reason ? el("div", { class: "secondary" }, e.reason) : null,
    ),
    el("span", { class: `badge ${e.verdict}` }, e.verdict),
  ))));
}

// --- actions ---

async function act(question, path, done) {
  if (question && !confirm(question)) return;
  try {
    await api(path, "POST");
    toast(done);
  } catch (error) {
    toast(String(error.message ?? error), true);
  }
  await refresh();
  const selected = state.accounts.find((a) => a.id === state.selected);
  if (selected && path.startsWith("/orders/")) await openAccount(selected);
}

async function refresh() {
  if (state.stopped) return;
  try {
    await Promise.all([overview(), approvals(), audit()]);
  } catch (error) {
    if (!state.stopped) toast(String(error.message ?? error), true);
  }
}

document.addEventListener("DOMContentLoaded", () => {
  if (!secret) {
    gate("Open the dashboard by running `ws-mcp dashboard` in a terminal.");
    return;
  }
  document.getElementById("kill").addEventListener("click", () => act(null, "/kill", "Trading stopped. No order can be previewed, placed, or cancelled."));
  document.getElementById("resume").addEventListener("click", () => act("Allow trading again?", "/resume", "Trading allowed again."));
  document.getElementById("refresh").addEventListener("click", () => accounts().catch((e) => toast(String(e.message ?? e), true)));
  refresh();
  accounts().catch((e) => show("accounts", el("p", { class: "inline-error" }, String(e.message ?? e))));
  setInterval(refresh, POLL_MS);
});
