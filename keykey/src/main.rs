#![no_main]
#![no_std]

use core::{
    panic::PanicInfo,
    sync::atomic::{compiler_fence, Ordering},
};
use cortex_m::asm;
use debouncer::{BtnState, PortDebouncer};
use embedded_hal::digital::v2::OutputPin;
use keyberon::{
    hid::HidClass,
    key_code::{KbHidReport, KeyCode},
    keyboard::Keyboard,
};
use rtic::app;
use rtt_target::{rprintln, rtt_init_print};
use stm32f1xx_hal::{
    pac,
    prelude::*,
    timer::{CountDownTimer, Event, Timer},
    usb::{Peripheral as UsbPeripheral, UsbBus, UsbBusType},
};
use typenum::consts::*;
use usb_device::{bus, class::UsbClass, prelude::*};

type UsbType = UsbDevice<'static, UsbBus<UsbPeripheral>>;
type KeyboardType = HidClass<'static, UsbBus<UsbPeripheral>, Keyboard<()>>;

#[app(device = stm32f1xx_hal::pac, peripherals = true)]
const APP: () = {
    struct Resources {
        debouncer_timer: CountDownTimer<pac::TIM2>,
        debouncer_handler: PortDebouncer<U8, U3>,
        usb_dev: UsbType,
        keyboard: KeyboardType,
    }

    #[init]
    fn init(cx: init::Context) -> init::LateResources {
        static mut USB_BUS: Option<bus::UsbBusAllocator<UsbBusType>> = None;

        let mut flash = cx.device.FLASH.constrain();
        let mut rcc = cx.device.RCC.constrain();
        let mut gpioa = cx.device.GPIOA.split(&mut rcc.apb2);

        let clocks = rcc
            .cfgr
            .use_hse(8.mhz())
            .sysclk(72.mhz())
            .pclk1(36.mhz())
            .freeze(&mut flash.acr);

        rtt_init_print!();
        assert!(clocks.usbclk_valid());

        // buttons, in order: shoot, left, right
        let _ = gpioa.pa0.into_pull_up_input(&mut gpioa.crl);
        let _ = gpioa.pa1.into_pull_up_input(&mut gpioa.crl);
        let _ = gpioa.pa2.into_pull_up_input(&mut gpioa.crl);

        // BluePill board has a pull-up resistor on the D+ line.
        // Pull the D+ pin down to send a RESET condition to the USB bus.
        // This forced reset is needed only for development, without it host
        // will not reset your device when you upload new firmware.
        let mut usb_dp = gpioa.pa12.into_push_pull_output(&mut gpioa.crh);
        usb_dp.set_low().ok();
        asm::delay(clocks.sysclk().0 / 100);

        let usb_dm = gpioa.pa11;
        let usb_dp = usb_dp.into_floating_input(&mut gpioa.crh);

        let usb = UsbPeripheral {
            usb: cx.device.USB,
            pin_dm: usb_dm,
            pin_dp: usb_dp,
        };

        *USB_BUS = Some(UsbBus::new(usb));

        let keyboard = keyberon::new_class(USB_BUS.as_ref().unwrap(), ());

        let usb_dev = UsbDeviceBuilder::new(USB_BUS.as_ref().unwrap(), UsbVidPid(0x16c0, 0x27dd))
            .manufacturer("Fake company")
            .product("KeyKey")
            .serial_number("TEST")
            .build();

        let mut timer2 =
            Timer::tim2(cx.device.TIM2, &clocks, &mut rcc.apb1).start_count_down(200.hz());
        timer2.listen(Event::Update);

        rprintln!("Init finished");

        init::LateResources {
            debouncer_timer: timer2,
            debouncer_handler: PortDebouncer::new(32, 208),
            usb_dev,
            keyboard,
        }
    }

    #[idle]
    fn idle(_cx: idle::Context) -> ! {
        loop {
            asm::nop();
        }
    }

    #[task(binds = TIM2, priority = 2, resources = [debouncer_timer, debouncer_handler, keyboard])]
    fn debouncer_task(mut cx: debouncer_task::Context) {
        cx.resources.debouncer_timer.clear_update_interrupt_flag();
        if cx
            .resources
            .debouncer_handler
            .update(!(unsafe { (*pac::GPIOA::ptr()).idr.read().bits() }))
        {
            let mut report = KbHidReport::default();

            let state = cx.resources.debouncer_handler.get_state(0);
            if let Ok(rot) = state {
                if rot == BtnState::ChangedToPressed {
                    report.pressed(KeyCode::A);
                }
            }

            let state = cx.resources.debouncer_handler.get_state(1);
            if let Ok(right) = state {
                if right == BtnState::ChangedToPressed {
                    report.pressed(KeyCode::B);
                }
            }

            let state = cx.resources.debouncer_handler.get_state(2);
            if let Ok(left) = state {
                if left == BtnState::ChangedToPressed {
                    report.pressed(KeyCode::C);
                }
            }

            cx.resources.keyboard.lock(|shared| {
                if shared.device_mut().set_keyboard_report(report.clone()) {
                    while let Ok(0) = shared.write(report.as_bytes()) {}
                }
            });
        }
    }

    #[task(binds = USB_LP_CAN_RX0, priority = 3, resources = [usb_dev, keyboard])]
    fn usb(cx: usb::Context) {
        if cx.resources.usb_dev.poll(&mut [cx.resources.keyboard]) {
            cx.resources.keyboard.poll();
        }
    }
};

#[inline(never)]
#[panic_handler]
fn panic(info: &PanicInfo) -> ! {
    cortex_m::interrupt::disable();
    rprintln!("{}", info);
    loop {
        compiler_fence(Ordering::SeqCst);
    }
}