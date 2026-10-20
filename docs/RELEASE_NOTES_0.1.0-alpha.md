# NAS Backup Manager 0.1.0-alpha

Первый альфа-билд бесплатного Open Source приложения для резервного копирования
папок на локальный диск, внешний накопитель или NAS.

## Что доступно

- Независимые задачи, копирование новых/изменённых файлов и архив версий.
- Предпросмотр, восстановление, защита от массового удаления, безопасная отмена.
- Ручной запуск и расписания Windows при закрытом интерфейсе.
- Прогресс и оставшееся время, трей, уведомления, русский и английский интерфейс.
- Portable и установщики x64/x86; язык установщика — русский или английский.
- При удалении установленной версии очищаются только её задания планировщика.

## Какой файл выбрать

| Файл | Для чего |
|---|---|
| `NAS-Backup-Manager-0.1.0-alpha-setup-x64.exe` | Установка на Windows x64 |
| `NAS-Backup-Manager-0.1.0-alpha-setup-x86.exe` | Установка 32-битной версии |
| `NAS-Backup-Manager-0.1.0-alpha-portable-x64.zip` | Portable x64: распакуйте и запустите EXE в portable-x64 |
| `NAS-Backup-Manager-0.1.0-alpha-portable-x86.zip` | Portable x86: распакуйте и запустите EXE в portable-x86 |
| `NAS-Backup-Manager-0.1.0-alpha-inno-scripts.zip` | Скрипты Inno Setup 7; для компиляции нужны также обе portable-папки рядом |
| `NAS-Backup-Manager-0.1.0-alpha-third-party-licenses.zip` | Метаданные зависимостей и доступные тексты лицензий |
| `SHA256SUMS.txt` | Контрольные суммы всех перечисленных файлов |

Нужны Microsoft Edge WebView2 Runtime и системный .NET Framework 4.x.
Node.js/Rust и отдельный Visual C++ Redistributable пользователю не требуются.
Сохраняйте три EXE portable-комплекта вместе. Для сборки Inno распакуйте архивы
скриптов и portable в одну папку, сохраняя имена portable-x64 и portable-x86.

## Статус проверки

Проверено на Windows 10 19045 x64; x86 — под WOW64. Нативная 32-битная Windows,
отдельно Windows 11, Linux/macOS и полноценные испытания реального NAS остаются
непроверенными. Сборки не подписаны Authenticode. Это **pre-release**: сначала
проверьте работу на тестовых папках. Архив имеет срок хранения; режим без архива
не позволяет восстановить удалённые или заменённые копии средствами приложения.

Документация: [README](https://github.com/redn1ghtz/NAS_Backup_Manager#readme).
Ошибки: [Issues](https://github.com/redn1ghtz/NAS_Backup_Manager/issues).

MIT License. Copyright (c) 2026 redn1ghtz (Vlad Ershov).

## English

First alpha release. Choose x64 or x86, installer or portable ZIP. Keep portable
executables together. Russian/English UI and installers are included. Windows
10 x64 was tested; x86 ran under WOW64. Native x86 Windows, Windows 11 separately,
Linux/macOS and real NAS validation remain open. WebView2 and the system .NET
Framework are required. Unsigned pre-release; test on disposable data first.
