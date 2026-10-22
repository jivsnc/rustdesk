use std::{
    collections::HashMap,
    iter::FromIterator,
    sync::{Arc, Mutex},
};

use sciter::Value;

use hbb_common::{
    allow_err,
    config::{LocalConfig, PeerConfig},
    log,
};

#[cfg(not(feature = "flutter"))]
use crate::ui_session_interface::Session;
use crate::{common::get_app_name, ipc, ui_interface::*};

mod cm;
#[cfg(feature = "inline")]
pub mod inline;
pub mod remote;

#[allow(dead_code)]
type Status = (i32, bool, i64, String);

lazy_static::lazy_static! {
    // stupid workaround for https://sciter.com/forums/topic/crash-on-latest-tis-mac-sdk-sometimes/
    static ref STUPID_VALUES: Mutex<Vec<Arc<Vec<Value>>>> = Default::default();
}

#[cfg(not(feature = "flutter"))]
lazy_static::lazy_static! {
    pub static ref CUR_SESSION: Arc<Mutex<Option<Session<remote::SciterHandler>>>> = Default::default();
}

struct UIHostHandler;

pub fn start(args: &mut [String]) {
    #[cfg(target_os = "macos")]
    crate::platform::delegate::show_dock();
    #[cfg(all(target_os = "linux", feature = "inline"))]
    {
        let app_dir = std::env::var("APPDIR").unwrap_or("".to_string());
        let mut so_path = "/usr/share/rustdesk/libsciter-gtk.so".to_owned();
        for (prefix, dir) in [
            ("", "/usr"),
            ("", "/app"),
            (&app_dir, "/usr"),
            (&app_dir, "/app"),
        ]
        .iter()
        {
            let path = format!("{prefix}{dir}/share/rustdesk/libsciter-gtk.so");
            if std::path::Path::new(&path).exists() {
                so_path = path;
                break;
            }
        }
        sciter::set_library(&so_path).ok();
    }
    #[cfg(windows)]
    // Check if there is a sciter.dll nearby.
    if let Ok(exe) = std::env::current_exe() {
        if let Some(parent) = exe.parent() {
            let sciter_dll_path = parent.join("sciter.dll");
            if sciter_dll_path.exists() {
                // Try to set the sciter dll.
                let p = sciter_dll_path.to_string_lossy().to_string();
                let res = sciter::set_library(&p);
                log::debug!("Found dll:{}, \n {:?}", p, res);
            }
        }
    }
    // https://github.com/c-smile/sciter-sdk/blob/master/include/sciter-x-types.h
    // https://github.com/rustdesk/rustdesk/issues/132#issuecomment-886069737
    #[cfg(windows)]
    allow_err!(sciter::set_options(sciter::RuntimeOptions::GfxLayer(
        sciter::GFX_LAYER::WARP
    )));
    use sciter::SCRIPT_RUNTIME_FEATURES::*;
    allow_err!(sciter::set_options(sciter::RuntimeOptions::ScriptFeatures(
        ALLOW_FILE_IO as u8 | ALLOW_SOCKET_IO as u8 | ALLOW_EVAL as u8 | ALLOW_SYSINFO as u8
    )));
    let mut frame = sciter::WindowBuilder::main_window().create();
    #[cfg(windows)]
    allow_err!(sciter::set_options(sciter::RuntimeOptions::UxTheming(true)));
    frame.set_title(&crate::get_app_name());
    #[cfg(target_os = "macos")]
    crate::platform::delegate::make_menubar(frame.get_host(), args.is_empty());
    #[cfg(windows)]
    crate::platform::try_set_window_foreground(frame.get_hwnd() as _);
    let page;
    if args.len() > 1 && args[0] == "--play" {
        args[0] = "--connect".to_owned();
        let path: std::path::PathBuf = (&args[1]).into();
        let id = path
            .file_stem()
            .map(|p| p.to_str().unwrap_or(""))
            .unwrap_or("")
            .to_owned();
        args[1] = id;
    }
    if args.is_empty() {
        std::thread::spawn(move || check_zombie());
        crate::common::check_software_update();
        frame.event_handler(UI {});
        frame.sciter_handler(UIHostHandler {});
        page = "index.html";
        // Start pulse audio local server.
        #[cfg(target_os = "linux")]
        std::thread::spawn(crate::ipc::start_pa);
    } else if args[0] == "--install" {
        frame.event_handler(UI {});
        frame.sciter_handler(UIHostHandler {});
        page = "install.html";
    } else if args[0] == "--cm" {
        frame.register_behavior("connection-manager", move || {
            Box::new(cm::SciterConnectionManager::new())
        });
        page = "cm.html";
        *cm::HIDE_CM.lock().unwrap() = crate::ipc::get_config("hide_cm")
            .ok()
            .flatten()
            .unwrap_or_default()
            == "true";
    } else if (args[0] == "--connect"
        || args[0] == "--file-transfer"
        || args[0] == "--port-forward"
        || args[0] == "--rdp")
        && args.len() > 1
    {
        #[cfg(windows)]
        {
            let hw = frame.get_host().get_hwnd();
            crate::platform::windows::enable_lowlevel_keyboard(hw as _);
        }
        let mut iter = args.iter();
        let Some(cmd) = iter.next() else {
            log::error!("Failed to get cmd arg");
            return;
        };
        let cmd = cmd.to_owned();
        let Some(id) = iter.next() else {
            log::error!("Failed to get id arg");
            return;
        };
        let id = id.to_owned();
        let pass = iter.next().unwrap_or(&"".to_owned()).clone();
        let args: Vec<String> = iter.map(|x| x.clone()).collect();
        frame.set_title(&id);
        frame.register_behavior("native-remote", move || {
            let handler =
                remote::SciterSession::new(cmd.clone(), id.clone(), pass.clone(), args.clone());
            #[cfg(not(feature = "flutter"))]
            {
                *CUR_SESSION.lock().unwrap() = Some(handler.inner());
            }
            Box::new(handler)
        });
        page = "remote.html";
    } else {
        log::error!("Wrong command: {:?}", args);
        return;
    }
    #[cfg(feature = "inline")]
    {
        let html = if page == "index.html" {
            inline::get_index()
        } else if page == "cm.html" {
            inline::get_cm()
        } else if page == "install.html" {
            inline::get_install()
        } else {
            inline::get_remote()
        };
        frame.load_html(html.as_bytes(), Some(page));
    }
    #[cfg(not(feature = "inline"))]
    frame.load_file(&format!(
        "file://{}/src/ui/{}",
        std::env::current_dir()
            .map(|c| c.display().to_string())
            .unwrap_or("".to_owned()),
        page
    ));
    let hide_cm = *cm::HIDE_CM.lock().unwrap();
    if !args.is_empty() && args[0] == "--cm" && hide_cm {
        // run_app calls expand(show) + run_loop, we use collapse(hide) + run_loop instead to create a hidden window
        frame.collapse(true);
        frame.run_loop();
        return;
    }
    frame.run_app();
}

struct UI {}

impl UI {
    fn recent_sessions_updated(&self) -> bool {
        recent_sessions_updated()
    }

    fn get_id(&self) -> String {
        ipc::get_id()
    }

    fn temporary_password(&mut self) -> String {
        temporary_password()
    }

    fn update_temporary_password(&self) {
        update_temporary_password()
    }

    fn set_permanent_password(&self, password: String) {
        let _ = set_permanent_password_with_result(password);
    }

    fn is_local_permanent_password_set(&self) -> bool {
        is_local_permanent_password_set()
    }

    fn is_permanent_password_set(&self) -> bool {
        is_permanent_password_set()
    }

    fn get_remote_id(&mut self) -> String {
        LocalConfig::get_remote_id()
    }

    fn set_remote_id(&mut self, id: String) {
        LocalConfig::set_remote_id(&id);
    }

    fn goto_install(&mut self) {
        goto_install();
    }

    fn install_me(&mut self, _options: String, _path: String) {
        install_me(_options, _path, false, false);
    }

    fn update_me(&self, _path: String) {
        update_me(_path);
    }

    fn run_without_install(&self) {
        run_without_install();
    }

    fn show_run_without_install(&self) -> bool {
        show_run_without_install()
    }

    fn get_license(&self) -> String {
        get_license()
    }

    fn get_option(&self, key: String) -> String {
        get_option(key)
    }

    fn get_local_option(&self, key: String) -> String {
        get_local_option(key)
    }

    fn set_local_option(&self, key: String, value: String) {
        set_local_option(key, value);
    }

    fn peer_has_password(&self, id: String) -> bool {
        peer_has_password(id)
    }

    fn forget_password(&self, id: String) {
        forget_password(id)
    }

    fn get_peer_option(&self, id: String, name: String) -> String {
        get_peer_option(id, name)
    }

    fn set_peer_option(&self, id: String, name: String, value: String) {
        set_peer_option(id, name, value)
    }

    fn using_public_server(&self) -> bool {
        crate::using_public_server()
    }

    fn is_incoming_only(&self) -> bool {
        hbb_common::config::is_incoming_only()
    }

    pub fn is_outgoing_only(&self) -> bool {
        hbb_common::config::is_outgoing_only()
    }

    pub fn is_custom_client(&self) -> bool {
        crate::common::is_custom_client()
    }

    pub fn is_disable_settings(&self) -> bool {
        hbb_common::config::is_disable_settings()
    }

    pub fn is_disable_account(&self) -> bool {
        hbb_common::config::is_disable_account()
    }

    pub fn is_disable_installation(&self) -> bool {
        hbb_common::config::is_disable_installation()
    }

    pub fn is_disable_ab(&self) -> bool {
        hbb_common::config::is_disable_ab()
    }

    fn get_options(&self) -> Value {
        let hashmap: HashMap<String, String> =
            serde_json::from_str(&get_options()).unwrap_or_default();
        let mut m = Value::map();
        for (k, v) in hashmap {
            m.set_item(k, v);
        }
        m
    }

    fn test_if_valid_server(&self, host: String, test_with_proxy: bool) -> String {
        test_if_valid_server(host, test_with_proxy)
    }

    fn get_sound_inputs(&self) -> Value {
        Value::from_iter(get_sound_inputs())
    }

    fn set_options(&self, v: Value) {
        let mut m = HashMap::new();
        for (k, v) in v.items() {
            if let Some(k) = k.as_string() {
                if let Some(v) = v.as_string() {
                    if !v.is_empty() {
                        m.insert(k, v);
                    }
                }
            }
        }
        set_options(m);
    }

    fn set_option(&self, key: String, value: String) {
        set_option(key, value);
    }

    fn install_path(&mut self) -> String {
        install_path()
    }

    fn install_options(&self) -> String {
        install_options()
    }

    fn get_socks(&self) -> Value {
        Value::from_iter(get_socks())
    }

    fn set_socks(&self, proxy: String, username: String, password: String) {
        set_socks(proxy, username, password)
    }

    fn is_installed(&self) -> bool {
        is_installed()
    }

    fn get_supported_privacy_mode_impls(&self) -> String {
        serde_json::to_string(&crate::privacy_mode::get_supported_privacy_mode_impl())
            .unwrap_or_default()
    }

    fn is_root(&self) -> bool {
        is_root()
    }

    fn is_release(&self) -> bool {
        #[cfg(not(debug_assertions))]
        return true;
        #[cfg(debug_assertions)]
        return false;
    }

    fn is_share_rdp(&self) -> bool {
        is_share_rdp()
    }

    fn set_share_rdp(&self, _enable: bool) {
        set_share_rdp(_enable);
    }

    fn is_installed_lower_version(&self) -> bool {
        is_installed_lower_version()
    }

    fn closing(&mut self, x: i32, y: i32, w: i32, h: i32) {
        crate::server::input_service::fix_key_down_timeout_at_exit();
        LocalConfig::set_size(x, y, w, h);
    }

    fn get_size(&mut self) -> Value {
        let s = LocalConfig::get_size();
        let mut v = Vec::new();
        v.push(s.0);
        v.push(s.1);
        v.push(s.2);
        v.push(s.3);
        Value::from_iter(v)
    }

    fn get_mouse_time(&self) -> f64 {
        get_mouse_time()
    }

    fn check_mouse_time(&self) {
        check_mouse_time()
    }

    fn get_connect_status(&mut self) -> Value {
        let mut v = Value::array(0);
        let x = get_connect_status();
        v.push(x.status_num);
        v.push(x.key_confirmed);
        v.push(x.id);
        v
    }

    #[inline]
    fn get_peer_value(id: String, p: PeerConfig) -> Value {
        let values = vec![
            id,
            p.info.username.clone(),
            p.info.hostname.clone(),
            p.info.platform.clone(),
            p.options.get("alias").unwrap_or(&"".to_owned()).to_owned(),
        ];
        Value::from_iter(values)
    }

    fn get_peer(&self, id: String) -> Value {
        let c = get_peer(id.clone());
        Self::get_peer_value(id, c)
    }

    fn get_fav(&self) -> Value {
        Value::from_iter(get_fav())
    }

    fn store_fav(&self, fav: Value) {
        let mut tmp = vec![];
        fav.values().for_each(|v| {
            if let Some(v) = v.as_string() {
                if !v.is_empty() {
                    tmp.push(v);
                }
            }
        });
        store_fav(tmp);
    }

    fn get_recent_sessions(&mut self) -> Value {
        // to-do: limit number of recent sessions, and remove old peer file
        let peers: Vec<Value> = PeerConfig::peers(None)
            .drain(..)
            .map(|p| Self::get_peer_value(p.0, p.2))
            .collect();
        Value::from_iter(peers)
    }

    fn get_icon(&mut self) -> String {
        get_icon()
    }

    fn remove_peer(&mut self, id: String) {
        PeerConfig::remove(&id);
    }

    fn remove_discovered(&mut self, id: String) {
        remove_discovered(id);
    }

    fn send_wol(&mut self, id: String) {
        crate::lan::send_wol(id)
    }

    fn new_remote(&mut self, id: String, remote_type: String, force_relay: bool) {
        new_remote(id, remote_type, force_relay)
    }

    fn is_process_trusted(&mut self, _prompt: bool) -> bool {
        is_process_trusted(_prompt)
    }

    fn is_can_screen_recording(&mut self, _prompt: bool) -> bool {
        is_can_screen_recording(_prompt)
    }

    fn is_installed_daemon(&mut self, _prompt: bool) -> bool {
        is_installed_daemon(_prompt)
    }

    fn get_error(&mut self) -> String {
        get_error()
    }

    fn is_login_wayland(&mut self) -> bool {
        is_login_wayland()
    }

    fn current_is_wayland(&mut self) -> bool {
        current_is_wayland()
    }

    fn get_software_update_url(&self) -> String {
        crate::SOFTWARE_UPDATE_URL.lock().unwrap().clone()
    }

    fn get_new_version(&self) -> String {
        get_new_version()
    }

    fn get_version(&self) -> String {
        get_version()
    }

    fn get_fingerprint(&self) -> String {
        get_fingerprint()
    }

    fn get_app_name(&self) -> String {
        get_app_name()
    }

    fn get_software_ext(&self) -> String {
        #[cfg(windows)]
        let p = "exe";
        #[cfg(target_os = "macos")]
        let p = "dmg";
        #[cfg(target_os = "linux")]
        let p = "deb";
        p.to_owned()
    }

    fn get_software_store_path(&self) -> String {
        let mut p = std::env::temp_dir();
        let name = crate::SOFTWARE_UPDATE_URL
            .lock()
            .unwrap()
            .split("/")
            .last()
            .map(|x| x.to_owned())
            .unwrap_or(crate::get_app_name());
        p.push(name);
        format!("{}.{}", p.to_string_lossy(), self.get_software_ext())
    }

    fn create_shortcut(&self, _id: String) {
        #[cfg(windows)]
        create_shortcut(_id)
    }

    fn discover(&self) {
        std::thread::spawn(move || {
            allow_err!(crate::lan::discover());
        });
    }

    fn get_lan_peers(&self) -> String {
        // let peers = get_lan_peers()
        //     .into_iter()
        //     .map(|mut peer| {
        //         (
        //             peer.remove("id").unwrap_or_default(),
        //             peer.remove("username").unwrap_or_default(),
        //             peer.remove("hostname").unwrap_or_default(),
        //             peer.remove("platform").unwrap_or_default(),
        //         )
        //     })
        //     .collect::<Vec<(String, String, String, String)>>();
        serde_json::to_string(&get_lan_peers()).unwrap_or_default()
    }

    fn get_uuid(&self) -> String {
        get_uuid()
    }

    fn open_url(&self, url: String) {
        #[cfg(windows)]
        let p = "explorer";
        #[cfg(target_os = "macos")]
        let p = "open";
        #[cfg(target_os = "linux")]
        let p = if std::path::Path::new("/usr/bin/firefox").exists() {
            "firefox"
        } else {
            "xdg-open"
        };
        allow_err!(std::process::Command::new(p).arg(url).spawn());
    }

    fn change_id(&self, id: String) {
        reset_async_job_status();
        let old_id = self.get_id();
        change_id_shared(id, old_id);
    }

    fn http_request(&self, url: String, method: String, body: Option<String>, header: String) {
        http_request(url, method, body, header)
    }

    fn post_request(&self, url: String, body: String, header: String) {
        post_request(url, body, header)
    }

    fn is_ok_change_id(&self) -> bool {
        hbb_common::machine_uid::get().is_ok()
    }

    fn get_async_job_status(&self) -> String {
        get_async_job_status()
    }

    fn get_http_status(&self, url: String) -> Option<String> {
        get_async_http_status(url)
    }

    fn t(&self, name: String) -> String {
        crate::client::translate(name)
    }

    fn is_xfce(&self) -> bool {
        crate::platform::is_xfce()
    }

    fn get_api_server(&self) -> String {
        get_api_server()
    }

    fn has_hwcodec(&self) -> bool {
        has_hwcodec()
    }

    fn has_vram(&self) -> bool {
        has_vram()
    }

    fn get_langs(&self) -> String {
        get_langs()
    }

    fn video_save_directory(&self, root: bool) -> String {
        video_save_directory(root)
    }

    fn handle_relay_id(&self, id: String) -> String {
        handle_relay_id(&id).to_owned()
    }

    fn get_login_device_info(&self) -> String {
        get_login_device_info_json()
    }

    fn support_remove_wallpaper(&self) -> bool {
        support_remove_wallpaper()
    }

    fn has_valid_2fa(&self) -> bool {
        has_valid_2fa()
    }

    fn generate2fa(&self) -> String {
        generate2fa()
    }

    pub fn verify2fa(&self, code: String) -> bool {
        verify2fa(code)
    }

    fn verify_login(&self, raw: String, id: String) -> bool {
        crate::verify_login(&raw, &id)
    }

    fn generate_2fa_img_src(&self, data: String) -> String {
        let v = qrcode_generator::to_png_to_vec(data, qrcode_generator::QrCodeEcc::Low, 128)
            .unwrap_or_default();
        let s = hbb_common::sodiumoxide::base64::encode(
            v,
            hbb_common::sodiumoxide::base64::Variant::Original,
        );
        format!("data:image/png;base64,{s}")
    }

    pub fn check_hwcodec(&self) {
        check_hwcodec()
    }

    fn is_option_fixed(&self, key: String) -> bool {
        crate::ui_interface::is_option_fixed(&key)
    }

    fn get_builtin_option(&self, key: String) -> String {
        crate::ui_interface::get_builtin_option(&key)
    }

    fn is_remote_modify_enabled_by_control_permissions(&self) -> String {
        match crate::ui_interface::is_remote_modify_enabled_by_control_permissions() {
            Some(true) => "true",
            Some(false) => "false",
            None => "",
        }
        .to_string()
    }
}

impl sciter::EventHandler for UI {
    sciter::dispatch_script_call! {
        fn t(String);
        fn get_api_server();
        fn is_xfce();
        fn using_public_server();
        fn is_custom_client();
        fn is_outgoing_only();
        fn is_incoming_only();
        fn is_disable_settings();
        fn is_disable_account();
        fn is_disable_installation();
        fn is_disable_ab();
        fn get_id();
        fn temporary_password();
        fn update_temporary_password();
        fn set_permanent_password(String);
        fn is_local_permanent_password_set();
        fn is_permanent_password_set();
        fn get_remote_id();
        fn set_remote_id(String);
        fn closing(i32, i32, i32, i32);
        fn get_size();
        fn new_remote(String, String, bool);
        fn send_wol(String);
        fn remove_peer(String);
        fn remove_discovered(String);
        fn get_connect_status();
        fn get_mouse_time();
        fn check_mouse_time();
        fn get_recent_sessions();
        fn get_peer(String);
        fn get_fav();
        fn store_fav(Value);
        fn recent_sessions_updated();
        fn get_icon();
        fn install_me(String, String);
        fn is_installed();
        fn get_supported_privacy_mode_impls();
        fn is_root();
        fn is_release();
        fn set_socks(String, String, String);
        fn get_socks();
        fn is_share_rdp();
        fn set_share_rdp(bool);
        fn is_installed_lower_version();
        fn install_path();
        fn install_options();
        fn goto_install();
        fn is_process_trusted(bool);
        fn is_can_screen_recording(bool);
        fn is_installed_daemon(bool);
        fn get_error();
        fn is_login_wayland();
        fn current_is_wayland();
        fn get_options();
        fn get_option(String);
        fn get_local_option(String);
        fn set_local_option(String, String);
        fn get_peer_option(String, String);
        fn peer_has_password(String);
        fn forget_password(String);
        fn set_peer_option(String, String, String);
        fn get_license();
        fn test_if_valid_server(String, bool);
        fn get_sound_inputs();
        fn set_options(Value);
        fn set_option(String, String);
        fn get_software_update_url();
        fn get_new_version();
        fn get_version();
        fn get_fingerprint();
        fn update_me(String);
        fn show_run_without_install();
        fn run_without_install();
        fn get_app_name();
        fn get_software_store_path();
        fn get_software_ext();
        fn open_url(String);
        fn change_id(String);
        fn get_async_job_status();
        fn post_request(String, String, String);
        fn is_ok_change_id();
        fn create_shortcut(String);
        fn discover();
        fn get_lan_peers();
        fn get_uuid();
        fn has_hwcodec();
        fn has_vram();
        fn get_langs();
        fn video_save_directory(bool);
        fn handle_relay_id(String);
        fn get_login_device_info();
        fn support_remove_wallpaper();
        fn has_valid_2fa();
        fn generate2fa();
        fn generate_2fa_img_src(String);
        fn verify2fa(String);
        fn check_hwcodec();
        fn verify_login(String, String);
        fn is_option_fixed(String);
        fn get_builtin_option(String);
        fn is_remote_modify_enabled_by_control_permissions();
    }
}

impl sciter::host::HostHandler for UIHostHandler {
    fn on_graphics_critical_failure(&mut self) {
        log::error!("Critical rendering error: e.g. DirectX gfx driver error. Most probably bad gfx drivers.");
    }
}

#[cfg(not(target_os = "linux"))]
fn get_sound_inputs() -> Vec<String> {
    let mut out = Vec::new();
    use cpal::traits::{DeviceTrait, HostTrait};
    let host = cpal::default_host();
    if let Ok(devices) = host.devices() {
        for device in devices {
            if device.default_input_config().is_err() {
                continue;
            }
            if let Ok(name) = device.name() {
                out.push(name);
            }
        }
    }
    out
}

#[cfg(target_os = "linux")]
fn get_sound_inputs() -> Vec<String> {
    crate::platform::linux::get_pa_sources()
        .drain(..)
        .map(|x| x.1)
        .collect()
}

// sacrifice some memory
pub fn value_crash_workaround(values: &[Value]) -> Arc<Vec<Value>> {
    let persist = Arc::new(values.to_vec());
    STUPID_VALUES.lock().unwrap().push(persist.clone());
    persist
}

pub fn get_icon() -> String {
    // 128x128
    #[cfg(target_os = "macos")]
    // 128x128 on 160x160 canvas, then shrink to 128, mac looks better with padding
    {
        "data:image/png;base64,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".into()
    }
    #[cfg(not(target_os = "macos"))] // 128x128 no padding
    {
        "data:image/png;base64,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".into()
    }
}
