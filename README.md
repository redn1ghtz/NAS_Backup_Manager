# NAS Backup Manager — бесплатная программа для резервного копирования на NAS

**NAS Backup Manager** — бесплатная программа с открытым исходным кодом (Open Source) для резервного копирования (бэкапа) файлов и папок на **NAS, сетевое хранилище, локальный или внешний диск** в Windows. Поддерживает автоматическое резервное копирование по расписанию, одностороннее зеркалирование, архив предыдущих версий и восстановление файлов.

**Free NAS backup software for Windows.** NAS Backup Manager is an open-source application for scheduled one-way file and folder backups to NAS shares, network drives, local folders and external storage. It supports optional file version retention and restore.

Интерфейс доступен на русском и английском языках. Расписание выполняется при закрытом интерфейсе через Планировщик заданий Windows. **Текущая версия — 0.2.0-alpha:** перед использованием важных данных проверьте программу на тестовых папках. Windows 11 — целевая платформа, но отдельная проверка ещё требуется.

**Версия:** `0.2.0-alpha` · **Лицензия:** [MIT](LICENSE)

[Релизы](https://github.com/redn1ghtz/NAS_Backup_Manager/releases) ·
[Сообщить об ошибке](https://github.com/redn1ghtz/NAS_Backup_Manager/issues) ·
[Автор: redn1ghtz — Vlad Ershov](https://github.com/redn1ghtz)

> Альфа-версия. Перед использованием важных данных проверьте приложение на
> тестовых папках. Это одностороннее зеркало с опциональным архивом версий, а не
> двусторонняя синхронизация или согласованный снимок работающей базы данных.

## Возможности

- Независимые задачи без искусственного ограничения количества.
- Рекурсивное копирование новых и изменённых файлов; неизменённые пропускаются.
- Назначение: локальная папка, внешний диск, сетевая папка / NAS или облако OneDrive/Dropbox через rclone.
- SHA-256-проверка целостности, прогресс проверенных данных и отмена проверки.
- Клиентское шифрование age, создание ключа восстановления и расшифровка файлов.
- Повторы при временных сбоях с экспоненциальной задержкой.
- Отчёты Telegram и SMTP на языке интерфейса: результат, объём, длительность и дата завершения.
- Общие настройки Telegram/почты в настройках приложения; выбор каналов отдельно в каждой задаче.
- Архив удалённых и заменённых файлов на заданное число дней, восстановление
  с проверкой конфликтов и очистка истёкших версий.
- Режим без архива с подтверждением необратимого удаления и замены копий.
- Ручной запуск, ежедневное расписание, интервалы в минутах/часах/днях и запуск
  при входе пользователя в Windows.
- Настройка пропущенных запусков и пробуждения компьютера из сна.
- Предпросмотр, прогресс, оценка оставшегося времени, безопасная отмена,
  история операций и сведения о свободном месте назначения.
- Защита от массового удаления с настраиваемым минимальным количеством файлов.
- Значок трея и отключаемые уведомления Windows о начале, завершении и ошибках.
- Открытие интерфейса из трея или уведомления.
- Доступ к NAS через средства Windows: сохранённые учётные данные или гостевой
  доступ, если он разрешён сервером и политиками системы.
- Без активации, платных функций, искусственного ограничения скорости и телеметрии.

Для локальных и сетевых задач данные копируются **прямо в выбранную папку назначения**.
В основном облачном режиме используется локальный рабочий кэш размером с копию,
затем файлы отправляются в OneDrive или Dropbox. Облачная квота показывается отдельно
от локального диска.
Служебная `.nas-backup-manager` хранит архив, временные файлы и метки владения;
она исключена из зеркалирования. Старые задачи автоматически не переносятся.

## Платформы

| Платформа | Статус |
|---|---|
| Windows 10 x64 | Сборка и тесты выполнены на Windows 10 19045 |
| Windows x86, 32 бита | Проверено под WOW64; сама 32-битная ОС ещё не проверена |
| Windows 11 | Целевая платформа; отдельная проверка ещё требуется |
| Linux и macOS | Адаптеры планировщика реализованы; сборки и проверка на этих ОС пока не предоставлены |

## Установка и portable-версия

Выберите сборку подходящей разрядности на [странице релизов](https://github.com/redn1ghtz/NAS_Backup_Manager/releases).
Если бинарники ещё не опубликованы, соберите их по инструкции ниже.

| Вариант | Назначение |
|---|---|
| `portable-x64` | Запуск без установки на Windows x64 |
| `portable-x86` | Запуск без установки на Windows x86; также работает под WOW64 |
| `setup-x64.exe` | Установка x64-приложения для текущего пользователя |
| `setup-x86.exe` | Установка x86-приложения для текущего пользователя |

Установщики предлагают **русский и английский язык**. По умолчанию приложение
устанавливается в `%LOCALAPPDATA%\Programs\NAS Backup Manager` без запроса прав
администратора. Для установленной версии выбирайте одну разрядность.

Для portable откройте `NAS Backup Manager.exe`. Рядом должны оставаться:

```text
NAS Backup Manager.exe
nas-backup.exe
nas-backup-notify.exe
portable.flag
tools/             age, age-keygen, rclone и их лицензии
licenses/          лицензии сторонних библиотек
```

CLI выполняет плановые задания без интерфейса, компонент `nas-backup-notify.exe`
отвечает за трей, уведомления и открытие окна. Не смешивайте компоненты разных сборок.

### Зависимости для запуска

- Windows 10/11 подходящей разрядности.
- [Microsoft Edge WebView2 Runtime](https://developer.microsoft.com/microsoft-edge/webview2/).
- Системный .NET Framework 4.x для трея и уведомлений.
- Штатный Планировщик заданий Windows для автоматического запуска.

Node.js, npm, Rust, Visual Studio и отдельная установка SQLite пользователю
**не нужны**. Текущие сборки не требуют отдельного Visual C++ Redistributable.
Установщик не включает WebView2 и не загружает его автоматически.
Утилиты age, age-keygen и rclone входят в комплект. Для Telegram/SMTP нужен
`curl.exe` (обычно есть в Windows 10/11; путь можно задать в настройках).

## Первый бэкап

1. Создайте задачу и выберите папку-источник.
2. Укажите тип назначения и отдельную пустую папку для копии.
3. Настройте расписание, архив и защиту от массового удаления.
4. Проверьте предпросмотр и подтвердите операции.
5. Дождитесь первого успешного ручного запуска: после него выбранное расписание
   может быть активировано. Отмена первого запуска не активирует расписание.

Назначение становится зеркалом источника: удаления в источнике отражаются в копии.
При включённом архиве предыдущие версии сохраняются на установленный срок, затем
могут быть удалены при проверенной очистке. Режим без архива не позволяет
восстановить удалённые или заменённые копии средствами приложения.

Подробнее: [руководство пользователя](docs/USER_GUIDE.md).

## Расписание и хранение данных

На Windows Планировщик заданий запускает headless CLI. Постоянная служба не
устанавливается, интерфейс можно закрыть. Начатая синхронизация продолжает работать
в фоне. Пользователь должен оставаться вошедшим в Windows, а NAS — быть доступным
в его сеансе. Пробуждение из сна зависит от задачи, оборудования и настроек Windows.
Выключенный компьютер приложение не включает. Пропущенные запуски обрабатываются
по выбранной политике.

Перед перемещением portable-папки отключите расписания и включите их заново
после переноса. Деинсталлятор удаляет только задания `NASBackupManager-*` с одним
действием запуска CLI из удаляемой папки. Чужие задания, задания других portable
копий, пользовательские бэкапы и архивы сохраняются.

| Режим | Настройки и история |
|---|---|
| Portable | `data/state.sqlite` рядом с EXE при наличии `portable.flag` |
| Установленная версия | `%APPDATA%\org.nasbackupmanager.desktop\state.sqlite` |

Пароли NAS не записываются в SQLite: используется хранилище учётных данных Windows.
Конфигурация между portable и установленной версией автоматически не переносится.
Удаление программы сохраняет настройки и историю, отключая в них расписания.
Облачные токены, пароль SMTP и токен Telegram защищены DPAPI текущего пользователя
Windows и не возвращаются в интерфейс. Перенос на другой ПК/профиль потребует повторной
настройки секретов. Закрытый ключ age сохраните отдельно: без него восстановление невозможно.

## Обновление с 0.1.0-alpha

1. Дождитесь завершения работы и отключите расписания. Закройте программу.
2. Сделайте отдельную копию всей папки настроек (`data/` для portable или каталога
   `%APPDATA%\org.nasbackupmanager.desktop` для установленной версии).
3. Для portable замените файлы приложения, `tools/` и `licenses/` из нового ZIP
   в прежней папке, сохранив `data/`. Для установленной версии запустите новый
   установщик поверх прежней установки.
4. Откройте программу, проверьте задачи и предпросмотр, снова включите расписания.

База обновляется до схемы 13. Обратный запуск 0.1.0-alpha с обновлённой базой
не поддерживается: для отката нужна сохранённая до обновления копия всей папки настроек.

## Сборка из исходников

Для Windows нужны Rust stable, Node.js LTS с npm, Visual C++ Build Tools с
компонентами x86/x64 и Windows SDK, WebView2 и компилятор C# из .NET Framework.
Для установщиков дополнительно нужен **Inno Setup 7**. Python 3 нужен для подготовки
лицензий/релиза и дополнительных тестов, а не для работы приложения.

Используйте существующие `Cargo.lock` и `apps/desktop/package-lock.json`.
Команды из корня репозитория, PowerShell:

```powershell
Push-Location apps/desktop
npm ci
Pop-Location

rustup target add i686-pc-windows-msvc
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-portable-tools.ps1 -Architecture x64
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-portable-tools.ps1 -Architecture x86 -BuildAgeX86
python scripts/collect-tool-notices.py
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-release.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-installers.ps1
```

Результат появится в `releases/0.2.0-alpha/`:

```text
portable-x64/
portable-x86/
setup-x64.iss
setup-x86.iss
setup-common.iss
installers/
```

Для сборки через интерфейс откройте нужный `setup-*.iss` в Inno Setup Compiler 7
и нажмите **F9**. Оставьте рядом общий `.iss` и portable-папки.
Бинарные комплекты не включаются в исходный репозиторий; их можно прикреплять
к GitHub Releases. Полная инструкция: [INSTALLER.md](docs/INSTALLER.md).

### Разработка и проверки

Для запуска интерфейса в режиме разработки:

```powershell
Push-Location apps/desktop
npm run tauri -- dev
Pop-Location
```

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
cargo test --workspace --target i686-pc-windows-msvc --locked
node --test tests/state-refresh.test.mjs tests/ui-format.test.mjs
```

Тесты, изменяющие задания ОС, запускаются отдельно на временных задачах и каталогах.
[Результаты проверок](docs/VALIDATION.md) · [Замеры производительности](docs/PERFORMANCE.md).

## Технологии и структура

Rust · Tauri 2 · React · TypeScript · Tailwind CSS · shadcn/ui · SQLite.

```text
apps/desktop/      Интерфейс React и оболочка Tauri
crates/core/      Планирование, копирование, архив и защита данных
crates/storage/   SQLite, настройки, доступ к NAS и уведомления
crates/scheduler/ Windows Task Scheduler, systemd user timers и launchd
crates/cli/       Headless-запуск и обслуживание установленной версии
installers/inno/  Скрипты Inno Setup 7
scripts/          Сборка, упаковка и дополнительные проверки
docs/             Руководства, архитектура и отчёты
```

[Архитектура](docs/architecture/README.md) · [Защита данных](docs/architecture/safety.md).

## Ограничения альфа-версии

Проверки на настоящих NAS, съёмных дисках, при аварийном отключении питания,
на 32-битной Windows, Linux и macOS ещё требуют отдельной валидации.
Облако требует доступного сервиса и входа пользователя; корпоративные аккаунты могут
требовать разрешения администратора. Реальные аккаунты OneDrive/Dropbox и доставка
Telegram/SMTP не входят в автоматические локальные тесты.
Архив имеет срок хранения. Согласованность открытых баз данных и файлов приложений
без специальных механизмов не гарантируется. Сборки пока не имеют подписи Authenticode.

Сообщая об ошибке в [Issues](https://github.com/redn1ghtz/NAS_Backup_Manager/issues),
укажите версию приложения, Windows и её разрядность, тип назначения, шаги
воспроизведения и текст ошибки из журнала.

## Лицензия

MIT License — [полный текст](LICENSE).

Copyright (c) 2026 redn1ghtz (Vlad Ershov)

Сторонние компоненты: [THIRD_PARTY_NOTICES.md](THIRD_PARTY_NOTICES.md).

## English

NAS Backup Manager is a free, open-source desktop application for scheduled
one-way folder backups to local drives, external storage and OS-accessible NAS
shares. It copies new and changed files, mirrors deletions with optional version
retention, supports restore, preview, progress, cancellation and independent jobs.
The application and Windows installers support Russian and English.

The current release is **0.2.0-alpha**. It adds OneDrive/Dropbox backups through a
local cache, age encryption/recovery keys, SHA-256 integrity verification, retries
and localized Telegram/email reports. Account settings are shared across jobs;
Windows DPAPI protects credentials for the current user. Keep the private recovery
key separately.

Windows x64 and x86 packages can be built
using the included scripts; x86 was tested under WOW64, not a native 32-bit OS.
Linux/macOS adapters exist but those platforms are not yet validated. WebView2
and the system .NET Framework are required on Windows. Scheduled backups use
Windows Task Scheduler with the GUI closed; no permanent service is installed.

See the [user guide](docs/USER_GUIDE.md), [installer/build instructions](docs/INSTALLER.md)
and [validation report](docs/VALIDATION.md). Licensed under MIT.
