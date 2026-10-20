# Участие в проекте

Для ошибок и предложений используйте
[Issues](https://github.com/redn1ghtz/NAS_Backup_Manager/issues).
Укажите версию приложения, разрядность Windows, тип назначения, шаги воспроизведения
и ожидаемое поведение. Уязвимости описывайте по [SECURITY.md](SECURITY.md).

Перед изменениями изучите [README](README.md),
[архитектуру](docs/architecture/README.md) и [защиту данных](docs/architecture/safety.md).
Используйте Cargo.lock и apps/desktop/package-lock.json, сохраняйте RU/EN переводы
и текущий Dark Minimal. Не добавляйте искусственные лимиты задач или скорости.

Проверки из корня репозитория:

```powershell
cargo fmt --all -- --check
cargo clippy --workspace --all-targets --locked -- -D warnings
cargo test --workspace --locked
node --test tests/state-refresh.test.mjs tests/ui-format.test.mjs
```

Frontend: `npm ci`, затем `npm run build` в apps/desktop.
Для x86 требуется `rustup target add i686-pc-windows-msvc` и проверки с
`--target i686-pc-windows-msvc`. Инструкции: [INSTALLER.md](docs/INSTALLER.md).

Для файловых операций используйте только временные каталоги. Изменения удаления,
архива, восстановления, проверки путей или учётных данных должны сопровождаться
проверками безопасности. Не запускайте интеграционные тесты над пользовательскими
копиями. Тесты планировщика явно включаются отдельно и обязаны удалять только свои
временные UUID-задания. В PR укажите фактически выполненные проверки и ОС.
