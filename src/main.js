const { invoke } = window.__TAURI__.tauri;
const { listen } = window.__TAURI__.event;

// CONFIGS & STATE
const GITHUB_CONFIG_URL = "https://cdn.jsdelivr.net/gh/battleverseiv-design/Battleverse@main/config.json";
const LAUNCHER_VERSION = "1.0";

let config = {
  username: "",
  auth_token: null,
  mc_dir: "",
  ram: "6"
};

let remoteConfig = {};
let isLaunching = false;
let isGameActive = false;
let actionType = "LAUNCH"; // LAUNCH, INSTALL, UPDATE, OUTDATED

// DOM ELEMENTS
const serverStatusText = document.getElementById("server-status-text");
const statusDot = document.getElementById("status-dot");
const newsContent = document.getElementById("news-content");
const mailList = document.getElementById("mail-list");
const mailCount = document.getElementById("mail-count");
const progressBar = document.getElementById("progress-bar");
const statusMessage = document.getElementById("status-message");
const usernameText = document.getElementById("username-text");
const userStatusText = document.getElementById("user-status-text");
const authBtn = document.getElementById("auth-btn");
const ramDisplay = document.getElementById("ram-display");
const ramMinus = document.getElementById("ram-minus");
const ramPlus = document.getElementById("ram-plus");
const launchBtn = document.getElementById("launch-btn");
const btnWebsite = document.getElementById("btn-website");
const btnDiscord = document.getElementById("btn-discord");
const btnFolder = document.getElementById("btn-folder");

// MODALS DOM
const modalOverlay = document.getElementById("modal-overlay");
const authModal = document.getElementById("auth-modal");
const authClose = document.getElementById("auth-close");
const loginInput = document.getElementById("login-input");
const passwordInput = document.getElementById("password-input");
const authError = document.getElementById("auth-error");
const loginSubmitBtn = document.getElementById("login-submit-btn");
const registerBtn = document.getElementById("register-btn");

const mailModal = document.getElementById("mail-modal");
const mailClose = document.getElementById("mail-close");
const mailCloseBtn = document.getElementById("mail-close-btn");
const mailDetailSender = document.getElementById("mail-detail-sender");
const mailDetailSubject = document.getElementById("mail-detail-subject");
const mailDetailContent = document.getElementById("mail-detail-content");

// =========================================================
// INITIALIZATION
// =========================================================

async function init() {
  // 1. Load Local Settings
  config = await invoke("load_config");
  updateConfigUI();

  // 2. Fetch Remote Config
  try {
    const rawRemote = await invoke("fetch_remote_config", { url: GITHUB_CONFIG_URL });
    remoteConfig = rawRemote;
    
    // Update default values from remote if local empty
    if (!config.mc_dir) {
      config.mc_dir = remoteConfig.default_mc_dir || config.mc_dir;
      await saveConfig();
    }

    // Apply remote links
    btnWebsite.onclick = () => window.__TAURI__.shell.open(remoteConfig.web_site_url || "https://battleverseiv.netlify.app/");
    btnDiscord.onclick = () => window.__TAURI__.shell.open(remoteConfig.discord_url || "https://discord.gg/fQBgEpWZ4R");

    // Check version
    const remoteV = String(remoteConfig.launcher_version || LAUNCHER_VERSION);
    if (remoteV !== LAUNCHER_VERSION) {
      actionType = "OUTDATED";
      applyOutdatedUI();
      return;
    }

    // Update News
    renderNews();
    
    // Check server status
    checkServer(remoteConfig.server_ip || remoteConfig.server_address || "n6.joinserver.xyz:25792");

    // Check game files version on disk
    checkGameVersion();

  } catch (err) {
    console.error("Config fetch error:", err);
    statusMessage.innerText = "ОШИБКА ЗАГРУЗКИ КОНФИГУРАЦИИ";
    newsContent.innerHTML = `
      <h3 style="color: var(--danger)">Ошибка соединения</h3>
      <p>Не удалось получить данные конфигурации с GitHub. Проверьте подключение к интернету.</p>
      <p style="font-size: 11px; color: var(--text-dim); margin-top: 10px;">Детали: ${err}</p>
    `;
    // Try checking server status with default IP anyway
    checkServer("n6.joinserver.xyz:25792");
  }

  // 3. Load Mail Box
  pollMessages();
  setInterval(pollMessages, 60000);
}

// =========================================================
// UI UPDATERS
// =========================================================

function updateConfigUI() {
  ramDisplay.innerText = config.ram;
  
  if (config.auth_token) {
    usernameText.innerText = config.username.toUpperCase();
    userStatusText.innerText = "Авторизован в системе";
    authBtn.innerText = "ВЫЙТИ";
    authBtn.className = "btn btn-outline";
    authBtn.style.color = "var(--danger)";
    authBtn.style.borderColor = "rgba(239, 68, 68, 0.4)";
    authBtn.style.background = "rgba(239, 68, 68, 0.08)";
  } else {
    usernameText.innerText = "ГОСТЬ";
    userStatusText.innerText = "Требуется авторизация";
    authBtn.innerText = "ВОЙТИ";
    authBtn.className = "btn btn-outline";
    authBtn.style.color = "";
    authBtn.style.borderColor = "";
    authBtn.style.background = "";
  }
}

function applyOutdatedUI() {
  statusMessage.innerText = "ДОСТУПНА НОВАЯ ВЕРСИЯ ЛАУНЧЕРА";
  statusMessage.style.color = "var(--accent)";
  
  const launcherDownload = remoteConfig.launcher_download || remoteConfig.web_site_url || "https://battleverseiv.netlify.app/";
  
  newsContent.innerHTML = `
    <h3 style="color: var(--accent)">Устаревшая версия лаунчера</h3>
    <p>Доступна новая версия лаунчера (v${remoteConfig.launcher_version}). Текущая версия: v${LAUNCHER_VERSION}.</p>
    <p>Пожалуйста, скачайте актуальную версию с нашего сайта, чтобы продолжить игру.</p>
  `;
  
  launchBtn.innerText = "СКАЧАТЬ";
  launchBtn.disabled = false;
  launchBtn.style.background = "var(--accent)";
  launchBtn.style.boxShadow = "0 0 20px rgba(0, 240, 255, 0.2)";
  
  launchBtn.onclick = () => {
    window.__TAURI__.shell.open(launcherDownload);
  };
}

function renderNews() {
  const greeting = remoteConfig.greeting || "Добро пожаловать в Battleverse!";
  const news = remoteConfig.news_text || "Синхронизация завершена успешно.";
  const events = remoteConfig.events_text || "Событий нет.";
  const mc_v = remoteConfig.mc_version || "1.20.1";
  const forge_v = remoteConfig.forge_version || "47.4.10";
  const min_ram = remoteConfig.min_ram || 4;
  const default_ram = remoteConfig.default_ram || 8;
  const ver = remoteConfig.version || "1.0";

  newsContent.innerHTML = `
    <h3>${greeting}</h3>
    <div style="margin-top: 10px;">
      <strong style="color: var(--accent); font-size: 11px; letter-spacing: 0.5px;">НОВОСТИ</strong>
      <p style="margin-top: 2px; margin-bottom: 12px; font-size: 13px;">${news}</p>
      
      <strong style="color: var(--accent); font-size: 11px; letter-spacing: 0.5px;">ИВЕНТЫ И РАСПИСАНИЕ</strong>
      <p style="margin-top: 2px; margin-bottom: 12px; font-size: 13px;">${events}</p>
      
      <strong style="color: var(--accent); font-size: 11px; letter-spacing: 0.5px;">ИНФОРМАЦИЯ О СБОРКЕ</strong>
      <ul style="margin-top: 4px; padding-left: 15px; font-size: 13px; color: var(--text-dim);">
        <li>Версия сборки: <span style="color: var(--text)">v${ver}</span></li>
        <li>Ядро игры: <span style="color: var(--text)">${mc_v} (Forge ${forge_v})</span></li>
        <li>Минимальное ОЗУ: <span style="color: var(--text)">${min_ram} ГБ</span> (Рекомендуется ${default_ram} ГБ)</li>
      </ul>
    </div>
  `;
}

// =========================================================
// FUNCTIONALITIES
// =========================================================

async function saveConfig() {
  await invoke("save_config", { config });
}

async function checkServer(ip) {
  try {
    const status = await invoke("check_server_status", { ip });
    if (status.online) {
      serverStatusText.innerText = "СЕРВЕР АКТИВЕН";
      serverStatusText.style.color = "var(--success)";
      statusDot.className = "status-dot online";
    } else {
      serverStatusText.innerText = "СЕРВЕР НЕДОСТУПЕН";
      serverStatusText.style.color = "var(--danger)";
      statusDot.className = "status-dot offline";
    }
  } catch (err) {
    serverStatusText.innerText = "ОШИБКА СВЯЗИ";
    serverStatusText.style.color = "var(--danger)";
    statusDot.className = "status-dot offline";
  }
}

async function checkGameVersion() {
  if (actionType === "OUTDATED") return;
  if (!config.auth_token) {
    launchBtn.disabled = true;
    launchBtn.innerText = "ТРЕБУЕТСЯ ВХОД";
    launchBtn.style.background = "";
    launchBtn.style.boxShadow = "";
    return;
  }

  launchBtn.disabled = false;
  launchBtn.style.background = "";
  launchBtn.style.boxShadow = "";

  try {
    const remoteV = remoteConfig.version || "1.0";
    const status = await invoke("check_local_version", { mcDir: config.mc_dir, remoteVersion: remoteV });
    actionType = status;
    
    if (actionType === "INSTALL") {
      launchBtn.innerText = "УСТАНОВИТЬ";
    } else if (actionType === "UPDATE") {
      launchBtn.innerText = "ОБНОВИТЬ";
    } else {
      launchBtn.innerText = "ЗАПУСТИТЬ";
    }
  } catch (err) {
    console.error("Version check error:", err);
    actionType = "INSTALL";
    launchBtn.innerText = "УСТАНОВИТЬ";
  }
}

async function pollMessages() {
  if (!config.auth_token) {
    mailList.innerHTML = `<div class="mail-placeholder">Авторизуйтесь в системе, чтобы просматривать почту.</div>`;
    mailCount.innerText = "0";
    mailCount.style.display = "none";
    return;
  }

  try {
    const messages = await invoke("fetch_messages", {
      apiUrl: remoteConfig.api_base_url || "https://battleverseiv.netlify.app/api",
      token: config.auth_token
    });

    if (messages && messages.length > 0) {
      mailCount.innerText = messages.length;
      mailCount.style.display = "block";
      
      let html = "";
      messages.forEach((msg, idx) => {
        const isLatest = idx === 0;
        html += `
          <div class="mail-item ${isLatest ? 'latest' : ''}" onclick="openMailDetail('${escapeHtml(msg.sender || 'Система')}', '${escapeHtml(msg.subject || 'Без темы')}', '${escapeHtml(msg.content || '')}')">
            <span class="mail-sender">${escapeHtml(msg.sender || 'Система')}</span>
            <div class="mail-subject">${escapeHtml(msg.subject || 'Без темы')}</div>
            <div class="mail-preview">${escapeHtml(msg.content || '')}</div>
          </div>
        `;
      });
      mailList.innerHTML = html;
    } else {
      mailCount.innerText = "0";
      mailCount.style.display = "none";
      mailList.innerHTML = `<div class="mail-placeholder">Ваш почтовый ящик пуст.</div>`;
    }
  } catch (err) {
    console.error("Messages fetch error:", err);
    mailList.innerHTML = `<div class="mail-placeholder" style="color: var(--danger)">Не удалось загрузить почту.</div>`;
  }
}

// =========================================================
// MODALS LOGIC
// =========================================================

function openModal(modal) {
  modalOverlay.classList.add("active");
  modal.classList.add("active");
}

function closeModal() {
  modalOverlay.classList.remove("active");
  authModal.classList.remove("active");
  mailModal.classList.remove("active");
  authError.innerText = "";
  loginInput.value = "";
  passwordInput.value = "";
}

function openMailDetail(sender, subject, content) {
  mailDetailSender.innerText = `От: ${sender}`;
  mailDetailSubject.innerText = subject;
  
  // Format urls to clickable tags
  const urlRegex = /(https?:\/\/[^\s]+)/g;
  const formattedContent = content.replace(urlRegex, (url) => {
    return `<a href="#" onclick="window.__TAURI__.shell.open('${url}')">${url}</a>`;
  });
  
  mailDetailContent.innerHTML = formattedContent;
  openModal(mailModal);
}

// =========================================================
// EVENT LISTENERS
// =========================================================

authBtn.onclick = () => {
  if (config.auth_token) {
    // Logout logic
    config.auth_token = null;
    config.username = "";
    saveConfig();
    updateConfigUI();
    pollMessages();
    checkGameVersion();
  } else {
    openModal(authModal);
  }
};

authClose.onclick = closeModal;
mailClose.onclick = closeModal;
mailCloseBtn.onclick = closeModal;

modalOverlay.onclick = (e) => {
  if (e.target === modalOverlay) closeModal();
};

loginSubmitBtn.onclick = async () => {
  const username = loginInput.value.trim();
  const password = passwordInput.value.trim();

  if (!username || !password) {
    authError.innerText = "Заполните все поля!";
    return;
  }

  authError.innerText = "";
  loginSubmitBtn.disabled = true;
  loginSubmitBtn.innerText = "ПРОВЕРКА...";

  try {
    const res = await invoke("login_request", {
      apiUrl: remoteConfig.api_base_url || "https://battleverseiv.netlify.app/api",
      payload: { username, password }
    });

    config.auth_token = res.token;
    config.username = res.username;
    await saveConfig();
    
    closeModal();
    updateConfigUI();
    pollMessages();
    checkGameVersion();
  } catch (err) {
    authError.innerText = err;
  } finally {
    loginSubmitBtn.disabled = false;
    loginSubmitBtn.innerText = "ВОЙТИ";
  }
};

registerBtn.onclick = () => {
  window.__TAURI__.shell.open(remoteConfig.web_site_url || "https://battleverseiv.netlify.app/");
};

// RAM Buttons
ramMinus.onclick = async () => {
  let val = parseInt(config.ram);
  if (val > 2) {
    config.ram = String(val - 1);
    await saveConfig();
    updateConfigUI();
  }
};

ramPlus.onclick = async () => {
  let val = parseInt(config.ram);
  if (val < 32) {
    config.ram = String(val + 1);
    await saveConfig();
    updateConfigUI();
  }
};

// Folder Button
btnFolder.onclick = async () => {
  try {
    const newDir = await invoke("select_game_directory", { currentDir: config.mc_dir });
    if (newDir && newDir !== config.mc_dir) {
      config.mc_dir = newDir;
      await saveConfig();
      checkGameVersion();
    }
  } catch (err) {
    console.error("Directory selection error:", err);
  }
};

// Launch Game Button
launchBtn.onclick = async () => {
  if (actionType === "OUTDATED") return;
  if (isLaunching || isGameActive) return;

  const minRam = parseInt(remoteConfig.min_ram || "4");
  const selectedRam = parseInt(config.ram);

  if (selectedRam < minRam) {
    alert(`Для запуска сборки необходимо минимум ${minRam} ГБ ОЗУ!`);
    return;
  }

  isLaunching = true;
  launchBtn.disabled = true;
  launchBtn.innerText = "ПОДГОТОВКА...";

  try {
    // 1. Double check if local files exist
    const remoteV = remoteConfig.version || "1.0";
    const currentStatus = await invoke("check_local_version", { mcDir: config.mc_dir, remoteVersion: remoteV });
    if (currentStatus === "INSTALL" || currentStatus === "UPDATE") {
      actionType = currentStatus;
    }

    // 2. Install or update modpack if needed
    if (actionType === "INSTALL" || actionType === "UPDATE") {
      launchBtn.innerText = actionType === "INSTALL" ? "УСТАНОВКА..." : "ОБНОВЛЕНИЕ...";
      const ignoreFiles = remoteConfig.ignore_update_files || ["options.txt", "servers.dat"];
      await invoke("download_and_install_pack", {
        downloadUrl: remoteConfig.download_url,
        mcDir: config.mc_dir,
        ignoreList: ignoreFiles
      });
      
      // Save local version file
      await invoke("write_version_file", { mcDir: config.mc_dir, version: remoteV });
      actionType = "LAUNCH";
    }

    launchBtn.innerText = "ЗАПУСК...";

    // 3. Prepare join on API
    await invoke("prepare_join", {
      apiUrl: remoteConfig.api_base_url || "https://battleverseiv.netlify.app/api",
      token: config.auth_token
    });

    // 4. Launch Minecraft Java
    await invoke("launch_game", {
      mcDir: config.mc_dir,
      mcVersion: remoteConfig.mc_version || "1.20.1",
      forgeVersion: remoteConfig.forge_version || "47.4.10",
      username: config.username,
      ramGb: selectedRam
    });

  } catch (err) {
    console.error("Launch error:", err);
    alert(`Ошибка: ${err}`);
    isLaunching = false;
    checkGameVersion();
  }
};

// LISTENERS FOR RUST EVENTS
listen("download-progress", (event) => {
  const { status, val, max_val } = event.payload;
  statusMessage.innerText = status.toUpperCase();
  
  if (max_val > 0) {
    const pct = (val / max_val) * 100;
    progressBar.style.width = `${pct}%`;
  } else {
    progressBar.style.width = "0%";
  }
});

listen("game-active", (event) => {
  const active = event.payload;
  isGameActive = active;
  isLaunching = false;

  if (active) {
    launchBtn.disabled = true;
    launchBtn.innerText = "ИГРА ЗАПУЩЕНА";
    launchBtn.style.background = "var(--element-bg)";
    launchBtn.style.color = "var(--text-dim)";
    launchBtn.style.boxShadow = "none";
  } else {
    checkGameVersion();
    progressBar.style.width = "0%";
    statusMessage.innerText = "ОЖИДАНИЕ ДЕЙСТВИЙ";
  }
});

// ESCAPE HTML HELPER
function escapeHtml(str) {
  return str
    .replace(/&/g, "&amp;")
    .replace(/</g, "&lt;")
    .replace(/>/g, "&gt;")
    .replace(/"/g, "&quot;")
    .replace(/'/g, "&#039;");
}

// Launch Init
init();
