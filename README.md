# Battleverse Launcher

Официальный лаунчер Battleverse (Tauri + Rust + HTML/JS/CSS).

## Структура проекта
- `src/` — Пользовательский интерфейс (UI) лаунчера: HTML, CSS, JavaScript, ассеты.
- `src-tauri/` — Бэкенд лаунчера на Rust (запуск Minecraft, авторизация, загрузка обновлений, генератор NSIS инсталлятора).
- `mc_helper.py` — Вспомогательные скрипты запуска.

## Сборка из исходников
1. Установите [Rust](https://rustup.rs/) (edition 2021).
2. Установите `tauri-cli`:
   ```bash
   cargo install tauri-cli --version "^1.5"
   ```
3. Перейдите в папку бэкенда:
   ```bash
   cd src-tauri
   ```
4. Для тестирования в режиме разработки:
   ```bash
   cargo tauri dev
   ```
5. Для сборки готового инсталлятора (.exe):
   ```bash
   cargo tauri build
   ```
   Готовый файл `.exe` появится в `src-tauri/target/release/bundle/nsis/`.

## Публикация на GitHub
Выполните команды в корне этой папки:
```bash
git init
git add .
git commit -m "Initial commit of Battleverse Launcher"
git branch -M main
git remote add origin https://github.com/<ВАШ_АККАУНТ>/<ИМЯ_РЕПОЗИТОРИЯ>.git
git push -u origin main
```
