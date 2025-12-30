#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

use std::{cell::RefCell, rc::Rc, sync::OnceLock, time::Duration};

use anyhow::{Result, anyhow};
use i_slint_backend_winit::WinitWindowAccessor;
use lumen_compositor::{LiquidGlassConfig, LiquidGlassEngine};
use raw_window_handle::HasWindowHandle;
use single_instance::SingleInstance;
use slint::{ComponentHandle, Timer, TimerMode};
use windows::Win32::{
    Graphics::Gdi::{GetMonitorInfoW, MONITOR_DEFAULTTOPRIMARY, MONITORINFO, MonitorFromWindow},
    UI::WindowsAndMessaging::{HWND_TOPMOST, SWP_FRAMECHANGED, SWP_NOZORDER, SetWindowPos},
};

use crate::{mode::IslandVisualMode, utils::setup_window_style};

slint::include_modules!();

mod mode;
mod utils;

// ------------------- GLOBALS -------------------

static UI_WEAK: OnceLock<slint::Weak<LumenOverlay>> = OnceLock::new();

thread_local! {
    static LIQUID_GLASS_ENGINE: RefCell<Option<Rc<RefCell<LiquidGlassEngine<'static>>>>> =
        RefCell::new(None);
}

// ---------------------- MAIN ----------------------

fn main() -> Result<()> {
    let instance = SingleInstance::new("io.risuleia.lumen").unwrap();
    if !instance.is_single() {
        return Err(anyhow!("Already running"));
    }

    slint::platform::set_platform(Box::new(i_slint_backend_winit::Backend::new().unwrap()))?;

    let ui = LumenOverlay::new().unwrap();
    let weak = ui.as_weak();
    let _ = UI_WEAK.set(weak.clone());

    // ---------------- CORE THREAD ----------------
    std::thread::spawn(|| {
        let rt = tokio::runtime::Runtime::new().unwrap();

        rt.block_on(async {
            let mut core = lumen_core::IslandCore::new();
            let mut rx = core.subscribe();

            tokio::spawn(async move {
                core.start().await;
            });

            while let Ok(event) = rx.recv().await {
                match event {
                    lumen_core::CoreEvent::StateChanged(state) => {
                        let _ = slint::invoke_from_event_loop(move || {
                            if let Some(ui) = UI_WEAK.get().and_then(|w| w.upgrade()) {
                                let visual = match state {
                                    lumen_core::IslandState::IdleDormant => {
                                        IslandVisualMode::Dormant
                                    }
                                    lumen_core::IslandState::BriefPulse(_) => {
                                        IslandVisualMode::Pulse
                                    }
                                    lumen_core::IslandState::ActiveWidget(_) => {
                                        IslandVisualMode::Activity
                                    }
                                    lumen_core::IslandState::PrivacyIndicator(_) => {
                                        IslandVisualMode::Privacy
                                    }
                                    lumen_core::IslandState::ControlCenter => {
                                        IslandVisualMode::Activity
                                    }
                                };

                                match state {
                                    lumen_core::IslandState::IdleDormant => ui.set_mode(IslandMode::Idle),
                                    lumen_core::IslandState::ActiveWidget(lumen_core::ActivityKind::Media) => {
                                        ui.set_mode(IslandMode::Media);
                                        println!("some song");
                                    }
                                    lumen_core::IslandState::PrivacyIndicator(lumen_core::PrivacyKind::Microphone) => {
                                        ui.set_mode(IslandMode::Mic);
                                        println!("mic received");
                                    }
                                    lumen_core::IslandState::PrivacyIndicator(lumen_core::PrivacyKind::Camera) => {
                                        ui.set_mode(IslandMode::Camera);
                                        println!("cam received");
                                    }
                                    _ => {}
                                }

                                LIQUID_GLASS_ENGINE.with(|slot| {
                                    if let Some(engine) = slot.borrow().as_ref() {
                                        let mut e = engine.borrow_mut();

                                        match visual {
                                            IslandVisualMode::Dormant => {
                                                e.motion.island.set_idle();
                                            }

                                            IslandVisualMode::Pulse => {
                                                e.motion.island.set_expanded(); // or custom pulse
                                            }

                                            IslandVisualMode::Activity => {
                                                e.motion.island.set_expanded();
                                            }

                                            IslandVisualMode::Privacy => {
                                                e.motion.island.set_expanded();
                                            }
                                        }
                                    }
                                });

                                // TODO: call compositor here:
                                // apply_visual_mode(visual);
                                println!("UI → VISUAL = {:?}", visual);
                            }
                        });
                    }

                    lumen_core::CoreEvent::VisualizerFrame(level) => {
                        let _ = slint::invoke_from_event_loop(move || {
                            LIQUID_GLASS_ENGINE.with(|slot| {
                                if let Some(engine) = slot.borrow().as_ref() {
                                    let mut e = engine.borrow_mut();

                                    // Drive motion
                                    // let m = &mut e.motion.island;

                                    // Subtle but alive
                                    let glow = 0.4 + (level * 0.6);
                                    let radius = 1.0 - (level * 0.2);
                                    let scale = 1.0 + (level * 0.12);

                                    // println!("{}-{}-{}", glow, radius, scale);

                                    // m.glow.set(glow);
                                    // m.radius.set(radius);
                                    // m.scale.set(scale);
                                }
                            });
                        });
                    }
                    lumen_core::CoreEvent::MicActive(level) => {
                        let _ = slint::invoke_from_event_loop(move || {
                            LIQUID_GLASS_ENGINE.with(|slot| {
                                if let Some(engine) = slot.borrow().as_ref() {
                                    let mut e = engine.borrow_mut();
                                    let m = &mut e.motion.island;

                                    m.glow.set(0.8 + level * 0.5);
                                    m.scale.set(1.05 + level * 0.1);
                                    m.radius.set(0.85);
                                }
                            });
                        });
                    }
                    lumen_core::CoreEvent::MicIdle => {
                        let _ = slint::invoke_from_event_loop(move || {
                            LIQUID_GLASS_ENGINE.with(|slot| {
                                if let Some(engine) = slot.borrow().as_ref() {
                                    let mut e = engine.borrow_mut();
                                    e.motion.island.set_idle();
                                }
                            });
                        });
                    }
                    lumen_core::CoreEvent::CameraActive => {
                        let _ = slint::invoke_from_event_loop(|| {
                            LIQUID_GLASS_ENGINE.with(|slot| {
                                if let Some(engine) = slot.borrow().as_ref() {
                                    let mut e = engine.borrow_mut();
                                    let m = &mut e.motion.island;

                                    m.scale.set(1.12);
                                    m.radius.set(0.75);
                                    m.glow.set(1.2);
                                }
                            });
                        });
                    }

                    lumen_core::CoreEvent::CameraIdle => {
                        let _ = slint::invoke_from_event_loop(|| {
                            LIQUID_GLASS_ENGINE.with(|slot| {
                                if let Some(engine) = slot.borrow().as_ref() {
                                    let mut e = engine.borrow_mut();
                                    e.motion.island.set_idle();
                                }
                            });
                        });
                    }

                    _ => {}
                }
            }
        });
    });

    // -------- LIQUID GLASS INIT ----------
    slint::Timer::single_shot(Duration::from_millis(60), move || {
        if let Some(ui) = weak.upgrade() {
            let window = ui.window();

            window.with_winit_window(|w| {
                if let Ok(handle) = w.window_handle() {
                    if let raw_window_handle::RawWindowHandle::Win32(h) = handle.as_raw() {
                        let hwnd =
                            windows::Win32::Foundation::HWND(h.hwnd.get() as isize as *mut _);

                        unsafe {
                            let width = 300;
                            let height = 60;

                            setup_window_style(hwnd, width, height);

                            let mut mi = MONITORINFO::default();
                            mi.cbSize = std::mem::size_of::<MONITORINFO>() as u32;

                            let mon = MonitorFromWindow(hwnd, MONITOR_DEFAULTTOPRIMARY);
                            let _ = GetMonitorInfoW(mon, &mut mi);

                            let screen_width = mi.rcWork.right - mi.rcWork.left;

                            let x = (screen_width / 2) - (width / 2);
                            let y = 20;

                            let _ = SetWindowPos(
                                hwnd,
                                Some(HWND_TOPMOST),
                                x,
                                y,
                                width,
                                height,
                                SWP_FRAMECHANGED | SWP_NOZORDER,
                            );

                            let window_static: &'static _ =
                                std::mem::transmute::<&_, &'static _>(w);

                            let engine = futures::executor::block_on(async {
                                LiquidGlassEngine::new(LiquidGlassConfig::default(), window_static)
                                    .await
                            })
                            .unwrap();

                            let rc = Rc::new(RefCell::new(engine));

                            LIQUID_GLASS_ENGINE.with(|slot| {
                                *slot.borrow_mut() = Some(rc.clone());
                            });
                        }
                    }
                } else {
                    eprintln!("FAILED: no window_handle()");
                }
            });
        } else {
            eprintln!("TIMER: ui already gone");
        }
    });

    // -------- ENGINE TICK ----------
    let tick_timer = Rc::new(RefCell::new(Timer::default()));

    tick_timer
        .borrow_mut()
        .start(TimerMode::Repeated, Duration::from_millis(16), move || {
            LIQUID_GLASS_ENGINE.with(|slot| {
                if let Some(engine) = slot.borrow().as_ref() {
                    engine.borrow_mut().tick();
                }
            });
        });

    println!("MAIN: running UI");
    ui.run()?;
    Ok(())
}
