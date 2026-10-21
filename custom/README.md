# Своя сборка RustDesk

Ветка `custom/<версия>` = официальный тег `<версия>` + наши коммиты.
Всё наше лежит в `custom/` либо оформлено отдельными небольшими коммитами,
чтобы при переходе на новый релиз конфликтов было как можно меньше.

## Где что меняется

| Что | Где | Нужна пересборка |
|---|---|---|
| Сервер, ключ сервера, API, relay | `custom/config.json` → `override-settings` | нет, только `custom.txt` |
| Название программы | `custom/config.json` → `app-name` | нет |
| Настройки по умолчанию (пользователь может поменять) | `custom/config.json` → `default-settings` | нет |
| Принудительные настройки (пользователь не может поменять) | `custom/config.json` → `override-settings` | нет |
| Иконки, логотип | отдельный коммит `custom: branding` | да |
| Поведение, которого нет в настройках | отдельный коммит на каждое изменение | да |

Имена настроек — ключи из `libs/base/src/config/keys.rs` с `_` → `-`
(например `enable-file-transfer`, `approve-mode`, `verification-method`).
Значения — строки, булевы как `"Y"` / `"N"`.

`app-name` обязателен и не может быть `RustDesk`: по нему программа понимает,
что она кастомный клиент, и отключает проверку официальных обновлений
(`check_software_update` в `src/common.rs`). Он же задаёт имя папки настроек
(`%APPDATA%\<app-name>`) и имя службы.

### custom.txt

Конфиг попадает в программу как `custom.txt` рядом с `rustdesk.exe`
(`load_custom_client` в `src/common.rs`). Файл подписан ed25519, программа
проверяет подпись публичным ключом `KEY` в `read_custom_client`. Коммит
`custom: trust our custom.txt signing key` заменяет ключ RustDesk на наш.

Секретный ключ: `C:\work\keys\rustdesk-custom-sign.key`. **Не класть в git.**
Потеря ключа = новый ключ + новый коммит с `KEY` + пересборка.

```
python custom\sign.py pubkey C:\work\keys\rustdesk-custom-sign.key
```

### Брендинг (Windows, Sciter)

* `res/icon.ico` — иконка exe и упакованного exe;
* `res/tray-icon.ico` — иконка в трее;
* `src/ui.rs`, `get_icon()`, ветка не-macOS — логотип в окне (PNG 128×128 в base64).

## Сборка

Инструменты лежат в `C:\work` (`rust`, `vcpkg`, `LLVM`, `python`), скачанные
`sciter.dll` и `usbmmidd_v2.zip` кэшируются в `C:\work\cache`.

```
powershell -ExecutionPolicy Bypass -File custom\build.ps1
```

Результат: `custom\out\dist\<app-name>-<версия>-x86_64-sciter.exe` — один exe,
внутри `<app-name>.exe` (в нижнем регистре), `sciter.dll`, `usbmmidd_v2` и
`custom.txt`; при запуске распаковывается в `%LOCALAPPDATA%\<app-name>`
(упаковщик берёт папку из имени вложенного exe).

Рядом — `…-x86_64-sciter.zip` для поставки в своём каталоге без упаковщика:
`<app-name>.exe`, `sciter.dll`, `custom.txt` (без него клиент — обычный
RustDesk с официальными серверами). Драйвер `usbmmidd_v2` туда не входит.

Внешний exe можно переименовать как угодно, кроме имён, на которые клиент
реагирует особо: с `install`, `-qs-`, `host=`, `licensed`.

### Имена вспомогательных файлов

Коммит `custom: name helper executables after SNCDesk` заменяет зашитые
`rustdesk` на `sncdesk`: `RuntimeBroker_sncdesk.exe` (режим конфиденциальности;
`src/privacy_mode/win_topmost_window.rs` и `libs/portable/src/main.rs` — имена
должны совпадать) и `%LOCALAPPDATA%\sncdesk-sciter\sncdesk.exe` (portable-служба,
`src/platform/windows.rs`). При смене `app-name` поправить и их.
Установщик MSI (`res/msi`) не используется и не менялся.

## Переход на новый официальный релиз

```
git fetch origin --tags
git switch -c custom/1.5.1 custom/1.5.0
git rebase --onto 1.5.1 1.5.0
```

* `git rebase` переносит наши коммиты на новый тег по одному. На конфликте:
  поправить файлы, `git add`, `git rebase --continue`. Если изменение
  уже не нужно (сделано в апстриме) — `git rebase --skip`.
* Проверить, что в `src/common.rs` ещё есть `read_custom_client` и `KEY`, а
  в CI (`.github/workflows/flutter-build.yml`) не поменялись `SCITER_RUST_VERSION`,
  `LLVM_VERSION`, `VCPKG_COMMIT_ID`. Если поменялись — обновить инструменты в
  `C:\work` и `RUSTUP_TOOLCHAIN` в `build.ps1`.
* Собрать `custom\build.ps1`, проверить, отправить ветку:
  `git push origin custom/1.5.1`.

Ветки прошлых версий не удаляются: по ним всегда можно пересобрать старый релиз.

## Правила для наших коммитов

* Один коммит — одно изменение, заголовок начинается с `custom:`.
* Всё, что можно сделать настройкой в `config.json`, делать настройкой.
* В исходниках менять минимум строк, новый код — в новых файлах.
  Чем меньше затронуто чужих строк, тем реже конфликты при rebase.
