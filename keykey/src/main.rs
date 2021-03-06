#![no_main]
#![no_std]

use core::{
    panic::PanicInfo,
    sync::atomic::{compiler_fence, Ordering},
};
use cortex_m::asm;
use debouncer::{
    typenum::{consts::*, Unsigned},
    PortDebouncer,
};
use embedded_hal::digital::v2::OutputPin;
use heapless::spsc::{Consumer, Queue};
use keylib::{packets::AppCommand, PID, VID};
use rtic::app;
use stm32f1xx_hal::{
    pac,
    prelude::*,
    timer::{CountDownTimer, Event, Timer},
    usb::{Peripheral as UsbPeripheral, UsbBus, UsbBusType},
};
use usb_device::{bus, class::UsbClass, prelude::*};

#[macro_use]
mod loggy;
mod flash;
mod keyboard;
use flash::{ConfigWriter, FlashError};
use keyboard::{Keykey, Matrix};

type UsbType = UsbDevice<'static, UsbBus<UsbPeripheral>>;
type KeyboardType = Keykey<'static, 'static, UsbBus<UsbPeripheral>>;
pub type BtnsType = U3;
pub const NUM_BTS: usize = BtnsType::USIZE;

#[app(device = stm32f1xx_hal::pac, peripherals = true)]
const APP: () = {
    struct Resources {
        debouncer_timer: CountDownTimer<pac::TIM2>,
        debouncer_handler: PortDebouncer<U8, BtnsType>,
        usb_dev: UsbType,
        keyboard: KeyboardType,
        app_consumer: Consumer<'static, AppCommand, U8>,
        matrix: Matrix,
        writer: ConfigWriter,
    }

    #[init]
    fn init(cx: init::Context) -> init::LateResources {
        static mut USB_BUS: Option<bus::UsbBusAllocator<UsbBusType>> = None;
        static mut Q: Queue<AppCommand, U8> = Queue(heapless::i::Queue::new());

        let mut flash = cx.device.FLASH.constrain();
        let mut rcc = cx.device.RCC.constrain();
        let mut gpioa = cx.device.GPIOA.split(&mut rcc.apb2);

        let clocks = rcc
            .cfgr
            .use_hse(8.mhz())
            .sysclk(72.mhz())
            .pclk1(36.mhz())
            .freeze(&mut flash.acr);

        init_log!();
        assert!(clocks.usbclk_valid());

        // buttons, in order: shoot, left, right
        let _ = gpioa.pa0.into_pull_up_input(&mut gpioa.crl);
        let _ = gpioa.pa1.into_pull_up_input(&mut gpioa.crl);
        let _ = gpioa.pa2.into_pull_up_input(&mut gpioa.crl);

        // Flash writer
        let writer = ConfigWriter::new(flash).unwrap();
        let matrix = writer.get_config().unwrap_or_else(Matrix::new);

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
        let (prod, cons) = Q.split();

        let keyboard = Keykey::new(USB_BUS.as_ref().unwrap(), prod);

        let usb_dev = UsbDeviceBuilder::new(USB_BUS.as_ref().unwrap(), UsbVidPid(VID, PID))
            .manufacturer("Fake company")
            .product("KeyKey")
            .serial_number("TEST")
            .build();

        let mut timer2 =
            Timer::tim2(cx.device.TIM2, &clocks, &mut rcc.apb1).start_count_down(200.hz());
        timer2.listen(Event::Update);

        log!("Init finished");

        init::LateResources {
            debouncer_timer: timer2,
            debouncer_handler: PortDebouncer::new(16, 96),
            usb_dev,
            keyboard,
            app_consumer: cons,
            writer,
            matrix,
        }
    }

    #[idle]
    fn idle(_cx: idle::Context) -> ! {
        loop {
            // This should change to `wfi` eventually, just leaving like this to ease development,
            // since it can be a bit harder to attach to the chip during wfi
            asm::nop();
        }
    }

    #[task(binds = TIM2, priority = 2, resources = [debouncer_timer, debouncer_handler, keyboard, matrix, app_consumer, writer])]
    fn debouncer_task(mut cx: debouncer_task::Context) {
        cx.resources.debouncer_timer.clear_update_interrupt_flag();
        if cx
            .resources
            .debouncer_handler
            .update(!(unsafe { (*pac::GPIOA::ptr()).idr.read().bits() }))
        {
            let report = cx.resources.matrix.update(cx.resources.debouncer_handler);

            cx.resources.keyboard.lock(|shared| {
                if shared.set_keyboard_report(report.clone()) {
                    if shared.write(report.as_bytes()).is_err() {
                        log!("Error while sending report");
                    }
                }
            });
        }
        // Update the layout if needed
        if let Some(cmd) = cx.resources.app_consumer.dequeue() {
            let writer = cx.resources.writer;
            if let Err(FlashError::FlashNotErased) = cx.resources.matrix.update_layout(cmd, writer)
            {
                // Something went wrong, erase the flash and try one more time
                writer.write_default().unwrap();
                cx.resources.matrix.update_layout(cmd, writer).unwrap();
            }
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
    log!("{}", info);
    loop {
        compiler_fence(Ordering::SeqCst);
    }
}
