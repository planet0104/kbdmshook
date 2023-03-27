use anyhow::{anyhow, Result};
use num_enum::TryFromPrimitive;
use once_cell::sync::Lazy;
use std::sync::RwLock;
use windows::Win32::{
    Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, WPARAM},
    UI::WindowsAndMessaging::{
        CallNextHookEx, DispatchMessageW, GetMessageW, SetWindowsHookExW, TranslateMessage,
        UnhookWindowsHookEx, HHOOK, HOOKPROC, KBDLLHOOKSTRUCT, MSG, MSLLHOOKSTRUCT, WH_KEYBOARD_LL,
        WH_MOUSE_LL,
    },
};

#[derive(Debug)]
pub enum KeyEvent {
    KeyPress(u32),
    KeyUp(u32),
}

#[derive(Default, PartialEq, Debug)]
pub struct Point {
    pub x: i32,
    pub y: i32,
}

#[derive(Debug, Eq, PartialEq, TryFromPrimitive)]
#[repr(i32)]
pub enum MouseEvent {
    MouseMove = 0x200,
    MouseLeftBUttonDown = 0x201,
    MouseLeftButtonUp = 0x202,
    MouseRightButtonDown = 0x204,
    MouseRightButtonUp = 0x205,
    MouseWheelRouting = 0x20A,
    MouseMiddleButtonDown = 0x2b,
    MouseMiddleButtonUp = 0x20c,
}

#[derive(Debug)]
pub enum Event {
    KeyEvent(KeyEvent),
    MouseEvent((MouseEvent, Point)),
}

type HookFn = unsafe extern "system" fn(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT;
type EventCallback = fn(Event);

static MOUSE_HOOK: Lazy<RwLock<Option<HHOOK>>> = Lazy::new(|| RwLock::new(None));
static KEYBOARD_HOOK: Lazy<RwLock<Option<HHOOK>>> = Lazy::new(|| RwLock::new(None));
static CALLBACK: Lazy<RwLock<Option<EventCallback>>> = Lazy::new(|| RwLock::new(None));
static EXIT: Lazy<RwLock<bool>> = Lazy::new(|| RwLock::new(false));

pub fn set_hook_callback(callback: EventCallback) -> Result<()> {
    CALLBACK
        .write()
        .map_err(|err| anyhow!("{:?}", err))?
        .replace(callback);
    Ok(())
}

pub fn start_hook_async(
    hook_mouse: bool,
    hook_keyboard: bool,
) -> std::thread::JoinHandle<std::result::Result<(), anyhow::Error>> {
    std::thread::spawn(move || start_hook(hook_mouse, hook_keyboard))
}

pub fn stop_hook() -> Result<()> {
    *EXIT.write().map_err(|err| anyhow!("{:?}", err))? = true;
    let _ = remove_keyboard_hook();
    let _ = remove_mouse_hook();
    Ok(())
}

pub fn start_hook(hook_mouse: bool, hook_keyboard: bool) -> Result<()> {
    if hook_keyboard {
        set_keyboard_hook(keyboard_hook_proc)?;
    }
    if hook_mouse {
        set_mouse_hook(mouse_hook_proc)?;
    }
    {
        *EXIT.write().map_err(|err| anyhow!("{:?}", err))? = false;
    }
    unsafe {
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, HWND(0), 0, 0).0 > 0 {
            let exit = *EXIT.read().map_err(|err| anyhow!("{:?}", err))?;
            if exit {
                break;
            }
            TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
    }
    Ok(())
}

unsafe extern "system" fn keyboard_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Ok(callback) = CALLBACK.read() {
        if let Some(callback) = callback.as_ref() {
            let data = lparam.0 as *const KBDLLHOOKSTRUCT;
            if !data.is_null() {
                let data: &KBDLLHOOKSTRUCT = &*data;
                match wparam {
                    WPARAM(0x100) => {
                        //普通键按下
                        callback(Event::KeyEvent(KeyEvent::KeyPress(data.vkCode)));
                    }
                    WPARAM(0x101) => {
                        //普通键抬起
                        callback(Event::KeyEvent(KeyEvent::KeyUp(data.vkCode)));
                    }
                    WPARAM(0x104) => {
                        //系统键按下
                        callback(Event::KeyEvent(KeyEvent::KeyPress(data.vkCode)));
                    }
                    WPARAM(0x105) => {
                        //系统键抬起
                        callback(Event::KeyEvent(KeyEvent::KeyUp(data.vkCode)));
                    }
                    _ => (),
                };
            }
        }
    }
    CallNextHookEx(*KEYBOARD_HOOK.read().unwrap(), code, wparam, lparam)
}

unsafe extern "system" fn mouse_hook_proc(code: i32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    if let Ok(callback) = CALLBACK.read() {
        if let Some(callback) = callback.as_ref() {
            let data = lparam.0 as *const MSLLHOOKSTRUCT;
            if !data.is_null() {
                if let Ok(mouse_event) = MouseEvent::try_from(wparam.0 as i32) {
                    let point = Point {
                        x: (*data).pt.x,
                        y: (*data).pt.y,
                    };
                    let _ = callback(Event::MouseEvent((mouse_event, point)));
                }
            }
        }
    }
    CallNextHookEx(*MOUSE_HOOK.read().unwrap(), code, wparam, lparam)
}

fn set_keyboard_hook(f: HookFn) -> Result<()> {
    let mut kbd_hook = KEYBOARD_HOOK.write().map_err(|err| anyhow!("{:?}", err))?;
    unsafe {
        kbd_hook.replace(SetWindowsHookExW(
            WH_KEYBOARD_LL,
            HOOKPROC::Some(f),
            HINSTANCE::default(),
            0,
        )?);
    }
    Ok(())
}

fn set_mouse_hook(f: HookFn) -> Result<()> {
    let mut ms_hook = MOUSE_HOOK.write().map_err(|err| anyhow!("{:?}", err))?;
    unsafe {
        ms_hook.replace(SetWindowsHookExW(
            WH_MOUSE_LL,
            HOOKPROC::Some(f),
            HINSTANCE::default(),
            0,
        )?);
    }
    Ok(())
}

fn remove_keyboard_hook() -> Result<()> {
    if let Some(hook) = KEYBOARD_HOOK
        .read()
        .map_err(|err| anyhow!("{:?}", err))?
        .as_ref()
    {
        unsafe {
            let _ = UnhookWindowsHookEx(*hook);
        }
    }
    Ok(())
}

fn remove_mouse_hook() -> Result<()> {
    if let Some(hook) = MOUSE_HOOK
        .read()
        .map_err(|err| anyhow!("{:?}", err))?
        .as_ref()
    {
        unsafe {
            let _ = UnhookWindowsHookEx(*hook);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn it_works() {
        let ret = start_hook_async(true, false);
        println!("{:?}", ret);
        let ret = set_hook_callback(|e| {
            println!("{:?}", e);
        });
        println!("{:?}", ret);

        std::thread::sleep(Duration::from_secs(10));
        let _ = stop_hook();
    }
}
