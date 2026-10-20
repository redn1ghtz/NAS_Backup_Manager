# Запуск без установки / Run without installing

Откройте `NAS Backup Manager.exe` в этой папке. Установщик не нужен.
Рядом должен оставаться `nas-backup.exe`: его вызывает планировщик при закрытом UI.
Также рядом нужен `nas-backup-notify.exe` — компонент значка трея, уведомлений
и открытия существующего окна. Он проверяется по SHA-256 и поставляется готовым:
программа не извлекает и не создаёт исполняемые файлы в `data` или временной папке.
Нужен установленный Microsoft Edge WebView2 Runtime.

Для 32-разрядной Windows 10 используйте отдельную папку `NAS Backup Manager x86`.
Обычная папка `NAS Backup Manager` содержит x64-версию. Не смешивайте EXE из разных
сборок: все три компонента должны принадлежать одному комплекту. x86-версия также
может работать на x64 Windows. Проверка под WOW64 не заменяет тест на x86 ОС.

Файл `portable.flag` включает хранение конфигурации и журнала в `data/state.sqlite`
рядом с EXE. Это **не** папка бэкапов: назначение выбирается в задаче.
Скрипт обновления EXE не удаляет `data/`. Закройте приложение перед обновлением.
Не перемещайте папку после регистрации расписания: сначала выключите расписание,
переместите папку и включите расписание повторно. При переименовании или удалении
всей папки сначала выключите расписания в UI.

Open `NAS Backup Manager.exe`; no installer is needed. Keep `nas-backup.exe` beside
it for headless schedules. WebView2 Runtime must already be installed. Application
notifications/tray and window activation also require the matching bundled
`nas-backup-notify.exe` alongside both executables. Its SHA-256 is verified; no
executable is unpacked into application data at runtime. Application
state lives in `data/` while `portable.flag` exists. Close the app before updating
executables. Disable and re-enable schedules after moving the application folder.

The separate `NAS Backup Manager x86` folder targets 32-bit Windows 10; the default
folder is x64. Keep all three executables from the same build. To reproduce x86:
install the Rust `i686-pc-windows-msvc` target and MSVC x86 build tools, then run
`powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-portable.ps1 -Architecture x86`.
The default command still builds x64. Existing lock files are preserved.

Windows Task Scheduler runs as the current signed-in user, without elevation or
stored Windows login passwords. Wake-from-sleep is an opt-in per-job setting. Network shares must be mounted and accessible
in that user's session. A sleeping/offline computer runs at the next opportunity
only with the configured missed-run policy. Test backup jobs on disposable folders
before adopting this build for valuable data; see the repository validation report.

Исходный код: https://github.com/redn1ghtz/NAS_Backup_Manager
Автор: https://github.com/redn1ghtz
