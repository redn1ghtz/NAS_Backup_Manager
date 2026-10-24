# NAS Backup Manager 0.2.0-alpha — сборка установщика

Альфа-версия: `0.2.0-alpha`. Числовая Windows-версия установщика — `0.2.0.0`.
В приложении и списке установленных программ отображается `0.2.0-alpha`.

Готовые комплекты находятся в `releases/0.2.0-alpha/`:

- `portable-x64/` — portable для Windows x64.
- `portable-x86/` — portable x86 (32 бита, также называемые x32).
- `setup-x64.iss` — скрипт Inno Setup 7 для x64-приложения.
- `setup-x86.iss` — скрипт Inno Setup 7 для x86-приложения.
- `setup-common.iss` — общий файл; оставьте рядом с обоими скриптами.

Оба установщика включают русский и английский язык с выбором при запуске.
Устанавливается одна копия приложения для текущего пользователя, без запроса
прав администратора, по умолчанию в `%LOCALAPPDATA%\Programs\NAS Backup Manager`.
При переходе между разрядностями используется та же установленная копия;
одновременная отдельная установка x86 и x64 не предусмотрена. Portable независимы.
Загрузчик обоих установщиков 32-битный для совместимости; полезная нагрузка x64
содержит только x64-компоненты. Это штатная возможность Inno Setup 7.

## Через окно Inno Setup 7

1. Установите Inno Setup 7. Готовым portable-комплектам Rust/Node.js не нужны.
2. Откройте `releases/0.2.0-alpha/setup-x64.iss` в Inno Setup Compiler.
3. Нажмите **Build → Compile** или **F9**.
4. Аналогично скомпилируйте `setup-x86.iss` для 32-битной версии.
5. EXE появятся в соседней папке `installers/`:
   `NAS-Backup-Manager-0.2.0-alpha-setup-x64.exe` и
   `NAS-Backup-Manager-0.2.0-alpha-setup-x86.exe`.

Для компиляции нужны соответствующие `portable-x64/`, `portable-x86/` и
`setup-common.iss` рядом со скриптами. Пользовательские `data/` не включаются.

## Командой из корня проекта

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-installers.ps1
```

Только одна разрядность:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-installers.ps1 -Architecture x86
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-installers.ps1 -Architecture x64
```

Либо непосредственно:

```powershell
& 'C:\Program Files\Inno Setup 7\ISCC.exe' '.\releases\0.2.0-alpha\setup-x64.iss'
& 'C:\Program Files\Inno Setup 7\ISCC.exe' '.\releases\0.2.0-alpha\setup-x86.iss'
```

## Пересборка из исходников

Нужны Node.js/npm, Rust stable, MSVC C++ Build Tools x86/x64, Windows SDK,
.NET Framework C# compiler и `rustup target add i686-pc-windows-msvc`.
Существующие package-lock.json и Cargo.lock сохраняются.
Для подготовки лицензий нужен Python 3.11+. Перед первой упаковкой скачайте
проверенные утилиты и соберите их notices:

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-portable-tools.ps1 -Architecture x64
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/prepare-portable-tools.ps1 -Architecture x86 -BuildAgeX86
python scripts/collect-tool-notices.py
```

Это загрузка инструментов сборки; пользователям готового установщика она не нужна.

```powershell
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/package-release.ps1
powershell -NoProfile -ExecutionPolicy Bypass -File scripts/build-installers.ps1
```

Для обновления release-папок уже собранными бинарниками используйте
`scripts/package-release.ps1 -SkipBuild`. Пересборка не переносит ваши portable
настройки в комплект поставки и не удаляет существующие данные.

## Установка, обновление и удаление

Нужны Windows 10/11 подходящей разрядности, Microsoft Edge WebView2 Runtime и
системный .NET Framework 4.x. WebView2 не включён и автоматически не загружается;
если его нет, установите с https://developer.microsoft.com/microsoft-edge/webview2/.
Node.js, Rust, SQLite и отдельный Visual C++ Redistributable пользователю не нужны.

Закройте интерфейс и дождитесь фоновых операций перед установкой/удалением.
Перед обновлением отключите расписания. Установщик не завершает worker принудительно.
Нельзя ставить установленную версию поверх папки с `portable.flag`.
Установленная версия хранит настройки/историю отдельно от EXE:
`%APPDATA%\org.nasbackupmanager.desktop\state.sqlite`; portable хранит их в `data/`.
Автоматического переноса конфигурации между этими режимами нет.

Деинсталлятор отключает и удаляет задачи в корне планировщика, чьё имя начинается
с `NASBackupManager-` **и** действие запускает `nas-backup.exe` именно из удаляемой
папки. Он также отмечает расписания этой установленной версии выключенными в БД.
Задачи Windows/других программ и задания других portable-папок не удаляются.
При активном worker или ошибке очистки удаление блокируется; повторите после
завершения операции. Часть заданий после ошибки может остаться отключённой.
Настройки, история, пароли в хранилище Windows, пользовательские бэкапы и архивы
не удаляются. Проверьте журнал и тестовый бэкап перед использованием альфы.

## English

Open `setup-x64.iss` or `setup-x86.iss` in Inno Setup 7 and press F9. Keep
`setup-common.iss` and both portable folders beside the scripts. Output EXEs are
written to `installers/`. Both installers offer Russian and English. Installation
is per-user. WebView2 is required separately. Uninstall removes only matching
NASBackupManager tasks pointing to this installation's CLI, disables schedule
flags, and preserves configuration, backup files and archives. Close active work
before uninstalling. x86 testing on x64 Windows does not validate a native x86 OS.
