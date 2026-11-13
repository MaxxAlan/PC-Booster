const { invoke } = window.__TAURI__.core;

const state = {
  view: "overview",
  tempSelected: new Set(),
  aiSelected: new Set(),
};

const $ = (id) => document.getElementById(id);
const fmtBytes = (n) => {
  if (!n) return "0 B";
  const units = ["B", "KB", "MB", "GB", "TB"];
  let value = n;
  let i = 0;
  while (value >= 1024 && i < units.length - 1) {
    value /= 1024;
    i++;
  }
  return `${i === 0 ? value : value.toFixed(1)} ${units[i]}`;
};

function setStatus(text, kind = "") {
  const el = $("status");
  el.textContent = text;
  el.className = `status ${kind}`;
}

async function call(cmd, args = {}) {
  try {
    return await invoke(cmd, args);
  } catch (err) {
    setStatus(String(err), "err");
    console.error(cmd, err);
    throw err;
  }
}

function table(headers, rows) {
  if (!rows.length) return '<p class="hint">Không có dữ liệu.</p>';
  const head = headers.map((h) => `<th>${h}</th>`).join("");
  const body = rows.map((r) => `<tr>${r}</tr>`).join("");
  return `<table><thead><tr>${head}</tr></thead><tbody>${body}</tbody></table>`;
}

// ---------------------------------------------------------------- navigation
const TITLES = {
  overview: "Tổng quan",
  startup: "Mục tự khởi động",
  services: "Dịch vụ tùy chọn",
  temp: "Dọn tệp tạm",
  ai: "Ứng dụng AI",
  processes: "Tiến trình",
  disk: "Ổ đĩa",
  defender: "Microsoft Defender",
  restore: "Khôi phục cấu hình",
  log: "Nhật ký",
};

async function showView(view) {
  state.view = view;
  document.querySelectorAll(".nav-item").forEach((b) => {
    b.classList.toggle("active", b.dataset.view === view);
  });
  document.querySelectorAll(".view").forEach((v) => {
    v.classList.toggle("hidden", v.id !== `view-${view}`);
  });
  $("view-title").textContent = TITLES[view] || view;
  await loadView(view);
}

async function loadView(view) {
  setStatus("đang tải…");
  try {
    if (view === "overview") await loadOverview();
    if (view === "startup") await loadStartup();
    if (view === "services") await loadServices();
    if (view === "temp") await loadTemp();
    if (view === "ai") await loadAi();
    if (view === "processes") await loadProcesses();
    if (view === "disk") await loadDisk();
    if (view === "defender") await loadDefender();
    if (view === "restore") await loadRestore();
    if (view === "log") await loadLog();
    setStatus("cập nhật lúc " + new Date().toLocaleTimeString());
  } catch (err) {
    setStatus("lỗi: " + err, "err");
  }
}

// ---------------------------------------------------------------- overview
async function loadOverview() {
  const o = await call("overview");
  const score = $("score");
  score.textContent = o.score + "/100";
  score.className =
    "score " + (o.score >= 75 ? "good" : o.score >= 50 ? "mid" : "bad");
  $("score-hint").textContent =
    o.notes.length ? o.notes[0] : "Máy đang ổn, không thấy vấn đề lớn.";

  $("ram").textContent = `${Math.round(o.memory.used_mb)} / ${Math.round(o.memory.total_mb)} MB`;
  const ramBar = $("ram-bar");
  ramBar.style.width = Math.min(100, o.memory.used_percent) + "%";
  ramBar.className = "bar-fill" + (o.memory.used_percent > 85 ? " danger" : "");

  const cDrive = o.drives.find((d) => d.path.toLowerCase() === "c:\\") || o.drives[0];
  if (cDrive) {
    $("disk-metric").textContent = fmtBytes(cDrive.free_bytes) + " trống";
    const used = cDrive.used_percent;
    const bar = $("disk-bar");
    bar.style.width = Math.min(100, used) + "%";
    bar.className = "bar-fill" + (used > 90 ? " danger" : used > 80 ? " warn" : "");
  }

  $("startup-metric").textContent = o.startup_count;
  $("temp-metric").textContent = fmtBytes(o.temp_old_bytes);
  $("ai-metric").textContent = o.ai_app_count;

  const notes = $("notes");
  notes.innerHTML = o.notes.length
    ? o.notes.map((n) => `<li>${n}</li>`).join("")
    : "<li>Không phát hiện vấn đề nào đáng chú ý.</li>";
}

// ---------------------------------------------------------------- startup
async function loadStartup() {
  const items = await call("startup_entries");
  const rows = items.map((s) => {
    const scope = s.user_scope ? "user" : "máy";
    return (
      `<td>${s.name}</td>` +
      `<td class="path">${s.command}</td>` +
      `<td>${s.location} <span class="badge badge-muted">${scope}</span></td>` +
      `<td><button class="btn btn-ghost btn-small" data-disable-startup="${s.name}" data-location="${s.location}">Tắt</button></td>`
    );
  });
  $("startup-list").innerHTML = table(["Tên", "Lệnh", "Vị trí", ""], rows);
}

// ---------------------------------------------------------------- services
async function loadServices() {
  const items = await call("optional_services");
  const rows = items.map((s) => {
    const badge = s.recommended
      ? '<span class="badge badge-ok">khuyến nghị tắt</span>'
      : '<span class="badge badge-warn">cân nhắc</span>';
    const button = s.exists
      ? `<button class="btn btn-ghost btn-small" data-disable-service="${s.name}">Tắt</button>`
      : '<span class="badge badge-muted">không có</span>';
    return (
      `<td><strong>${s.display_name}</strong><br><span class="path">${s.name} · ${s.impact}</span></td>` +
      `<td>${s.status}</td><td>${s.start_type}</td><td>${badge}</td><td>${button}</td>`
    );
  });
  $("services-list").innerHTML = table(
    ["Dịch vụ", "Trạng thái", "Khởi động", "Khuyến nghị", ""],
    rows
  );

  const schemes = await call("power_schemes");
  $("power-list").innerHTML = table(
    ["Gói nguồn", "GUID", "Đang dùng"],
    schemes.map(
      (s) =>
        `<td>${s.name}</td><td class="path">${s.guid}</td><td>${s.active ? "✓" : ""}</td>`
    )
  );
}

// ---------------------------------------------------------------- temp
async function loadTemp() {
  const items = await call("temp_candidates", { days: 7 });
  state.tempSelected = new Set();
  const total = items.reduce((sum, i) => sum + i.size_bytes, 0);
  const rows = items.map(
    (i) =>
      `<td><input type="checkbox" data-temp="${i.path}" /></td>` +
      `<td>${i.name}${i.is_directory ? " (thư mục)" : ""}</td>` +
      `<td class="num">${fmtBytes(i.size_bytes)}</td>` +
      `<td class="num">${i.age_days}</td>` +
      `<td class="path">${i.path}</td>`
  );
  $("temp-list").innerHTML =
    `<p class="hint">${items.length} mục, tổng ${fmtBytes(total)}</p>` +
    table(["", "Tên", "Dung lượng", "Tuổi (ngày)", "Đường dẫn"], rows);
}

// ---------------------------------------------------------------- ai
async function loadAi() {
  const items = await call("ai_apps");
  state.aiSelected = new Set();
  const rows = items.map(
    (a, index) =>
      `<td><input type="checkbox" data-ai="${index}" /></td>` +
      `<td><strong>${a.name}</strong><br><span class="path">${a.version} · ${a.publisher}</span></td>` +
      `<td><span class="badge badge-muted">${a.kind}</span></td>` +
      `<td class="num">${a.size_mb ? fmtBytes(a.size_mb * 1024 * 1024) : "-"}</td>` +
      `<td class="path">${(a.residual_paths || []).join("<br>") || "-"}</td>`
  );
  $("ai-list").innerHTML =
    (items.length
      ? `<p class="hint">${items.length} ứng dụng AI / coding agent được phát hiện.</p>`
      : `<p class="hint">Không tìm thấy ứng dụng AI nào.</p>`) +
    table(["", "Ứng dụng", "Nguồn", "Dung lượng", "Dữ liệu còn lại"], rows);
}

// ---------------------------------------------------------------- processes
async function loadProcesses() {
  const items = await call("running_processes");
  const rows = items
    .filter((p) => p.window_title || p.memory_mb > 50)
    .slice(0, 60)
    .map(
      (p) =>
        `<td>${p.name}</td>` +
        `<td class="path">${p.window_title || "-"}</td>` +
        `<td class="num">${p.cpu_percent}%</td>` +
        `<td class="num">${Math.round(p.memory_mb)} MB</td>` +
        `<td><button class="btn btn-ghost btn-small" data-close-pid="${p.pid}">Đóng</button></td>`
    );
  $("process-list").innerHTML = table(
    ["Tiến trình", "Cửa sổ", "CPU", "RAM", ""],
    rows
  );
}

// ---------------------------------------------------------------- disk
async function loadDisk() {
  const drives = await call("drives");
  $("drive-list").innerHTML = table(
    ["Ổ", "Đã dùng", "Tổng", "Trống", "Mức dùng"],
    drives.map(
      (d) =>
        `<td>${d.path}</td><td class="num">${fmtBytes(d.used_bytes)}</td>` +
        `<td class="num">${fmtBytes(d.total_bytes)}</td>` +
        `<td class="num">${fmtBytes(d.free_bytes)}</td>` +
        `<td class="num">${d.used_percent}%</td>`
    )
  );
}

// ---------------------------------------------------------------- defender
async function loadDefender() {
  const s = await call("defender_status");
  if (!s.available) {
    $("defender-status").innerHTML = `<p class="hint">${s.error}</p>`;
  } else {
    $("defender-status").innerHTML = table(
      ["Trạng thái", "Giá trị"],
      [
        `<td>Chế độ</td><td>${s.am_running_mode}</td>`,
        `<td>Antivirus</td><td>${s.antivirus_enabled ? "Bật" : "Tắt"}</td>`,
        `<td>Bảo vệ thời gian thực</td><td>${s.real_time_protection ? "Bật" : "Tắt"}</td>`,
        `<td>Chữ ký cập nhật</td><td>${s.signature_last_updated} (${s.signature_age_days} ngày)</td>`,
        `<td>Quét nhanh</td><td>${s.quick_scan_age_days} ngày trước</td>`,
        `<td>Quét toàn bộ</td><td>${s.full_scan_age_days} ngày trước</td>`,
      ]
    );
  }
  const detections = await call("defender_detections", { limit: 10 });
  $("defender-detections").innerHTML = table(
    ["Phát hiện", "Thời gian", "Đã xử lý", "Tài nguyên"],
    detections.map(
      (d) =>
        `<td>${d.threat_name}</td><td>${d.detected_at}</td>` +
        `<td>${d.action_success ? "✓" : "✗"}</td><td class="path">${d.resources}</td>`
    )
  );
}

// ---------------------------------------------------------------- restore
async function loadRestore() {
  const journal = await call("journal_state");
  const rows = [];
  (journal.services || []).forEach((s) =>
    rows.push(
      `<td>Dịch vụ</td><td>${s.name}</td><td>${s.previous_start_type} → ${s.was_running ? "đang chạy" : "đã dừng"}</td>`
    )
  );
  (journal.startup || []).forEach((s) =>
    rows.push(`<td>Khởi động</td><td>${s.name}</td><td class="path">${s.location}</td>`)
  );
  (journal.removed_apps || []).forEach((a) =>
    rows.push(`<td>Ứng dụng AI</td><td>${a.name}</td><td>${a.source} · ${a.removed_at}</td>`)
  );
  if (journal.power_plan) {
    rows.push(`<td>Gói nguồn</td><td class="path">${journal.power_plan}</td><td>đã đổi khỏi gói này</td>`);
  }
  $("journal-view").innerHTML =
    "<p class='hint'>Dữ liệu nằm tại %LOCALAPPDATA%\\PCBooster\\restore-state.json</p>" +
    (rows.length ? table(["Loại", "Mục", "Chi tiết"], rows) : "<p class='hint'>Chưa có thay đổi nào được ghi nhớ.</p>");
}

// ---------------------------------------------------------------- log
async function loadLog() {
  const lines = await call("app_log", { limit: 300 });
  $("log-view").textContent = lines.join("\n") || "Chưa có log.";
}

// ---------------------------------------------------------------- actions
document.addEventListener("click", async (event) => {
  const target = event.target;
  const nav = target.closest(".nav-item");
  if (nav) {
    showView(nav.dataset.view);
    return;
  }

  if (target.dataset.reload) {
    await loadView(target.dataset.reload);
    return;
  }

  if (target.dataset.disableStartup !== undefined) {
    try {
      const res = await call("disable_startup", {
        location: target.dataset.location,
        name: target.dataset.disableStartup,
      });
      setStatus(res, "ok");
      await loadStartup();
    } catch (err) {
      setStatus(String(err), "err");
    }
    return;
  }

  if (target.dataset.disableService !== undefined) {
    try {
      const res = await call("disable_service", { name: target.dataset.disableService });
      setStatus(res, "ok");
      await loadServices();
    } catch (err) {
      setStatus(String(err), "err");
    }
    return;
  }

  if (target.dataset.closePid !== undefined) {
    try {
      const res = await call("close_process", {
        pid: Number(target.dataset.closePid),
        graceful: true,
      });
      setStatus(res, "ok");
      await loadProcesses();
    } catch (err) {
      setStatus(String(err), "err");
    }
    return;
  }

  if (target.dataset.scan) {
    setStatus("đang gửi yêu cầu quét cho Defender…");
    try {
      const res = await call("defender_scan", { kind: target.dataset.scan, path: null });
      setStatus(res || "đã gửi yêu cầu quét", "ok");
    } catch (err) {
      setStatus(String(err), "err");
    }
    return;
  }

  if (target.id === "btn-refresh") {
    await loadView(state.view);
    return;
  }

  if (target.id === "btn-clean-temp") {
    const paths = Array.from(document.querySelectorAll("[data-temp]:checked")).map(
      (c) => c.dataset.temp
    );
    if (!paths.length) {
      setStatus("hãy chọn ít nhất một mục để dọn", "err");
      return;
    }
    try {
      const [removed, failed] = await call("clean_temp", { paths });
      setStatus(`đã chuyển ${removed} mục vào Thùng rác`, "ok");
      if (failed.length) console.warn("không xoá được", failed);
      await loadTemp();
    } catch (err) {
      setStatus(String(err), "err");
    }
    return;
  }

  if (target.id === "btn-remove-ai") {
    const items = await call("ai_apps");
    const indexes = Array.from(document.querySelectorAll("[data-ai]:checked")).map((c) =>
      Number(c.dataset.ai)
    );
    if (!indexes.length) {
      setStatus("hãy chọn ứng dụng cần gỡ", "err");
      return;
    }
    for (const index of indexes) {
      const app = items[index];
      if (!confirm(`Gỡ "${app.name}"?`)) continue;
      try {
        const res = await call("remove_ai_app", { app });
        setStatus(res, "ok");
      } catch (err) {
        setStatus(String(err), "err");
      }
    }
    await loadAi();
    return;
  }

  if (target.id === "btn-remove-ai-data") {
    const items = await call("ai_apps");
    const indexes = Array.from(document.querySelectorAll("[data-ai]:checked")).map((c) =>
      Number(c.dataset.ai)
    );
    const paths = [];
    indexes.forEach((i) => (items[i].residual_paths || []).forEach((p) => paths.push(p)));
    if (!paths.length) {
      setStatus("không có dữ liệu còn lại để xoá", "err");
      return;
    }
    if (!confirm(`Xoá vĩnh viễn ${paths.length} đường dẫn dữ liệu?`)) return;
    try {
      const [removed, failed] = await call("remove_ai_data", { paths });
      setStatus(`đã xoá ${removed} đường dẫn`, "ok");
      if (failed.length) console.warn(failed);
      await loadAi();
    } catch (err) {
      setStatus(String(err), "err");
    }
    return;
  }

  if (target.id === "btn-high-perf") {
    try {
      const res = await call("switch_high_performance");
      setStatus(`đã chuyển sang ${res}`, "ok");
      await loadServices();
    } catch (err) {
      setStatus(String(err), "err");
    }
    return;
  }

  if (target.id === "btn-restore") {
    try {
      const messages = await call("restore_all");
      $("restore-output").textContent = messages.join("\n");
      $("restore-output").classList.remove("hidden");
      setStatus("đã khôi phục", "ok");
    } catch (err) {
      setStatus(String(err), "err");
    }
    return;
  }

  if (target.id === "btn-elevate") {
    try {
      await call("relaunch_elevated");
      setStatus("đang mở cửa sổ UAC…", "ok");
    } catch (err) {
      setStatus(String(err), "err");
    }
    return;
  }

  if (target.id === "btn-largest") {
    setStatus("đang quét thư mục người dùng…");
    try {
      const home = await call("largest_folders", { root: "C:\\Users", limit: 20 });
      $("folder-list").innerHTML = table(
        ["Thư mục", "Dung lượng", "Số tệp"],
        home.map(
          (f) =>
            `<td class="path">${f.path}</td><td class="num">${fmtBytes(f.size_bytes)}</td>` +
            `<td class="num">${f.file_count}</td>`
        )
      );
      setStatus("xong");
    } catch (err) {
      setStatus(String(err), "err");
    }
    return;
  }

  if (target.id === "btn-selftest") {
    const out = $("selftest-output");
    out.classList.remove("hidden");
    out.textContent = "đang chạy…";
    try {
      const report = await call("bench", { iterations: 2 });
      out.textContent = report.cases
        .map((c) => `${c.name}: ${c.median_ms} ms (${c.items} mục)`)
        .join("\n");
    } catch (err) {
      out.textContent = String(err);
    }
  }
});

async function init() {
  try {
    const elevation = await call("elevation_state");
    const badge = $("elevation-badge");
    if (elevation.is_admin) {
      badge.textContent = "đang chạy với quyền quản trị";
      badge.className = "badge badge-ok";
    } else {
      badge.textContent = "chạy với quyền người dùng";
      badge.className = "badge badge-warn";
      $("btn-elevate").classList.remove("hidden");
    }
  } catch (err) {
    console.warn(err);
  }
  await showView("overview");
}

init();
