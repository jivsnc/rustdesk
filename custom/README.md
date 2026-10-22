# Своя сборка RustDesk

Ветка `custom/<версия>` = официальный тег `<версия>` + наши коммиты.
Всё наше лежит в `custom/` либо оформлено отдельными небольшими коммитами,
чтобы при переходе на новый релиз конфликтов было как можно меньше.

## Где что меняется

| Что | Где | Нужна пересборка |
|---|---|---|
| Сервер, ключ сервера, API, relay | конфиг → `override-settings` | нет, только `custom.txt` |
| Название программы | конфиг → `app-name` | нет |
| Настройки по умолчанию (пользователь может поменять) | конфиг → `default-settings` | нет |
| Принудительные настройки (пользователь не может поменять) | конфиг → `override-settings` | нет |
| Иконки, логотип | отдельный коммит `custom: branding` | да |
| Поведение, которого нет в настройках | отдельный коммит на каждое изменение | да |

Конфиг с адресом и ключом сервера в git не хранится: он лежит в
`C:\work\keys\sncdesk-config.json` рядом с ключом подписи (путь — параметр
`-Config` у `build.ps1`). В репозитории только шаблон `custom/config.example.json`.
Скрыт адрес только от git: в готовой программе `custom.txt` подписан, но не
зашифрован, и адрес с ключом сервера читаются из него открытым текстом.

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
RustDesk с официальными серверами), `LICENSE.txt`. Драйвер `usbmmidd_v2` туда не входит.

`LICENSE.txt` = `custom/NOTICE.txt` (что это изменённый RustDesk и где его исходники —
https://github.com/jivsnc/rustdesk) + `LICENCE` (AGPL-3.0). Лицензия требует давать
его вместе с программой; при новой версии поправить ветку в `NOTICE.txt`.

### 32-битная сборка (Windows 7 x86 и любая 32-битная Windows)

x86_64-сборка на 32-битной Windows не запускается. Для неё:

```
powershell -ExecutionPolicy Bypass -File custom\build.ps1 -Arch x86
```

Результат: `…-x86-sciter.exe` и `…-x86-sciter.zip`; работает и на 64-битной Windows.
Как в CI: тулчейн `nightly-2023-10-13-i686-pc-windows-msvc`, 32-битный LLVM
(`C:\work\LLVM-x86`), ветка `dyn_x86` у sciter-rs (`build.ps1` подменяет её
в `Cargo.toml`/`Cargo.lock` только на время сборки), 32-битный `sciter.dll`.
Кэш сборки — `custom\out\target-x86`, чтобы не пересобирать x64.

Один раз поставить инструменты и зависимости vcpkg (отдельный каталог: установка
в manifest-режиме удалила бы из `installed` пакеты другого triplet'а; `magnum-opus`
ищет их только в `%VCPKG_ROOT%\installed`, поэтому туда ставится junction;
`vcpkg install` запускать из корня репозитория):

```
rustup toolchain install nightly-2023-10-13-i686-pc-windows-msvc --target i686-pc-windows-msvc --profile minimal --force-non-host
7z x -oC:\work\LLVM-x86 LLVM-15.0.6-win32.exe -x!$PLUGINSDIR
set VCPKG_DEFAULT_HOST_TRIPLET=x86-windows-static
vcpkg install --triplet x86-windows-static --x-install-root=C:\work\vcpkg\installed-x86
mklink /J C:\work\vcpkg\installed\x86-windows-static C:\work\vcpkg\installed-x86\x86-windows-static
```

Внешний exe можно переименовать как угодно, кроме имён, на которые клиент
реагирует особо: с `install`, `-qs-`, `host=`, `licensed`.

### Имена вспомогательных файлов

Коммит `custom: name helper executables after SNCDesk` заменяет зашитые
`rustdesk` на `sncdesk`: `RuntimeBroker_sncdesk.exe` (режим конфиденциальности;
`src/privacy_mode/win_topmost_window.rs` и `libs/portable/src/main.rs` — имена
должны совпадать) и `%LOCALAPPDATA%\sncdesk-sciter\sncdesk.exe` (portable-служба,
`src/platform/windows.rs`). При смене `app-name` поправить и их.
Установщик MSI (`res/msi`) не используется и не менялся.

### Одно подтверждение на специалиста

При `approve-mode = click` каждое подключение спрашивает оператора, и специалист,
открывший передачу файлов рядом с управлением, получал второй запрос. Коммит
`custom: one approval per specialist` (`src/server/connection/sncdesk.rs` и
вызов в начале ветки подтверждения в `src/server/connection.rs`): пока у этого ID
есть принятое подключение управления или передачи файлов, следующее (кроме
туннеля портов и терминала) принимается без запроса. Окно подключения его
показывает, оператор может его закрыть. ID подключающийся сообщает сам.

### Логи и сторонние серверы

* Логи по умолчанию выключены (`core_main`, `src/core_main.rs`): если `RUST_LOG`
  не задана, ставится `off`. Включить — запустить клиент с `RUST_LOG=error`
  (или `debug`), логи пишутся в `%APPDATA%\<app-name>\log`.
* Проверка внешнего IPv6 через публичные STUN-серверы пропускается, когда
  IPv6-пробивка выключена (`test_ipv6`, `src/common.rs`); в конфиге
  `enable-webrtc` и `enable-ipv6-punch` = `N`.

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
* Всё, что можно сделать настройкой в конфиге, делать настройкой.
* В исходниках менять минимум строк, новый код — в новых файлах.
  Чем меньше затронуто чужих строк, тем реже конфликты при rebase.
