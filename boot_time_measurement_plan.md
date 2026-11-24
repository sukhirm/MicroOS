# Micro-OS: Boot-Time Measurement Plan & Repeatable Protocol

**Project:** Micro-OS Bare-Metal x86_64 Kernel  
**Date:** November 17, 2026  
**Target Hardware:** HP Victus 15-fb3xxx (AMD Ryzen 7 7445HS @ 3.2 GHz base)  
**Status:** Instrumentation Active & Verified  

---

## 1. Timing Data Measurement Methodology

In bare-metal operating system development, wall-clock time (RTC or NTP) is often unavailable or coarse-grained (e.g., standard CMOS RTC updates only once per second). To achieve microsecond- and cycle-accurate telemetry without fabricating performance numbers, Micro-OS implements a **dual-source measurement model**:

```
Micro-OS Dual-Source Boot Timing Architecture
┌──────────────────────────────────────────────┐     ┌─────────────────────────────────────────────┐
│ 1. Hardware RDTSC (Time Stamp Counter)       │     │ 2. 8254 PIT (Programmable Interval Timer)   │
│ - Available from instruction 1 of kernel_main│     │ - Initialized after PIC remapping at IRQ0   │
│ - Sub-nanosecond resolution (0.3125 ns/cycle)│     │ - Periodic 100 Hz interrupts (10 ms ticks)  │
│ - Monotonic per-core CPU cycle count         │     │ - Global atomic system uptime counter (ms)  │
└──────────────────────────────────────────────┘     └─────────────────────────────────────────────┘
```

### 1.1 CPU Cycle Calibration (AMD Ryzen 7 7445HS)
The AMD Ryzen 7 7445HS has a nominal base clock frequency ($f_{\text{base}}$) of **3.2 GHz** ($3.2 \times 10^9$ cycles per second):
$$\text{Period per cycle } (\tau) = \frac{1}{3.2 \times 10^9\text{ Hz}} = 0.3125\text{ ns}$$
$$\text{Time in milliseconds } (T_{\text{ms}}) = \frac{\Delta\text{Cycles}}{3,200,000}$$
$$\text{Time in microseconds } (T_{\mu\text{s}}) = \frac{\Delta\text{Cycles}}{3,200}$$

Under modern AMD Zen 4 processors, the `RDTSC` instruction reads an invariant, constant-rate Time Stamp Counter (`Pcnt` / Constant TSC), meaning cycle increments remain constant regardless of CPU dynamic P-state or thermal throttling.

---

## 2. In-Kernel Instrumentation Points

The kernel entry point ([kernel/src/main.rs](file:///C:/Users/sukhi/OneDrive/Documents/MicroOS_New/kernel/src/main.rs)) contains dedicated telemetry instrumentation logging actual cycle counts to the 16550 UART serial console:

```
kernel_main entry (t0: boot_start_tsc)
   │
   ├── GDT / TSS / IDT exception setup
   │
   ├── [Boot Timing 1] Memory Management & 1 MiB Heap (t1: mem_done_tsc)
   │   - Frame allocator initialized
   │   - Heap linked-list initialized & validated
   │
   ├── [Boot Timing 2] Task Scheduler & Multitasking (t2: sched_done_tsc)
   │   - 8259 PIC remapped & 8254 PIT programmed to 100 Hz
   │   - Preemptive multitasking demo executed
   │
   ├── [Boot Timing 3] Graphics & 16 MiB Backbuffer (t3: gfx_done_tsc)
   │   - UEFI GOP 1920x1080 linear framebuffer probed
   │   - 16 MiB virtual backbuffer mapping validated
   │
   ├── [Boot Timing 4] Desktop Shell & Window Manager (t4: desktop_done_tsc)
   │   - Cosmic Slate wallpaper, dock, status bar rendered
   │   - Window Manager demo windows Z-stacked
   │   - File Manager desktop application focused
   │
   └── [Boot Timing 5] Complete Boot to Desktop Event Loop (t5: total_boot_tsc)
       - PS/2 8042 keyboard & mouse input enabled
       - Interrupts enabled (sti)
       - Event dispatcher loop engaged (hlt)
```

---

## 3. Log Output Format Specification

During physical boot, the 16550 UART COM1 port (`0x3F8` at 115,200 baud, 8N1) and early boot splash log the following standard format:

```text
[INFO ] [kernel] [Boot Timing] 1. Memory Management & Heap Allocator initialized: <CYCLES> cycles (+<DELTA> cycles from startup)
[INFO ] [kernel] [Boot Timing] 2. Task Scheduler & Multitasking initialized: <CYCLES> cycles (+<DELTA> cycles)
[INFO ] [kernel] [Boot Timing] 3. Graphics Engine & 16 MiB Backbuffer initialized: <CYCLES> cycles (+<DELTA> cycles)
[INFO ] [kernel] [Boot Timing] 4. Desktop Shell & Window Manager ready: <CYCLES> cycles (+<DELTA> cycles)
[INFO ] [kernel] [Boot Timing] 5. Complete boot to interactive Desktop Event Loop: <TOTAL_CYCLES> cycles (PIT Timer: <MS> ms, <TICKS> ticks)
```

---

## 4. Physical Demonstration Measurement Protocol

To record repeatable boot timing benchmarks during your teacher evaluation:

### Equipment Setup:
1. **Primary Display (Visual):** Observe the on-screen splash and desktop appearance.
2. **Serial Telemetry Option (Precision):**
   - If connected via a USB-to-UART serial cable (RS232/TTL) to a second laptop running PuTTY/TeraTerm at `115200 8N1`, the exact timestamp strings will stream in real time.
3. **In-OS Telemetry (Live Verification):**
   - Once the desktop appears, click on the **Terminal** icon in the dock.
   - Run the command:
     ```bash
     user@micro-os:/home/user$ uptime
     up 0m 01s, 1 user, timer ticks: 114 (100 Hz 8254 PIT)
     ```
   - Run `meminfo` to verify allocation state:
     ```bash
     user@micro-os:/home/user$ meminfo
     ```

### Standardized Benchmarking Run Sheet (Template for Physical Demonstration):

| Boot Milestone | Measured CPU Cycles ($\Delta$) | Calculated Elapsed Time ($T = \frac{\Delta}{3.2\times 10^6}\text{ ms}$) | Expected Range |
| :--- | :--- | :--- | :--- |
| **1. Kernel Startup & CPU Diagnostics** | `[To be recorded on HP Victus]` | `[To be recorded]` | 1–5 ms |
| **2. Memory & 1 MiB Heap Allocator** | `[To be recorded on HP Victus]` | `[To be recorded]` | 2–8 ms |
| **3. Scheduler & Preemption Demo** | `[To be recorded on HP Victus]` | `[To be recorded]` | 50–150 ms (includes demo task sleep) |
| **4. 1920x1080 Framebuffer & 16 MiB Backbuffer** | `[To be recorded on HP Victus]` | `[To be recorded]` | 10–30 ms (VRAM copy & clearing) |
| **5. Desktop Shell & Window Manager** | `[To be recorded on HP Victus]` | `[To be recorded]` | 15–40 ms (Cosmic Slate UI composition) |
| **Total Cold Boot to Desktop Event Loop** | `[To be recorded on HP Victus]` | `[To be recorded]` | **< 300 ms** total kernel time |

*(Note: Total cold boot includes firmware POST time, which is controlled by HP BIOS (~3–5 seconds) before control is passed to `BOOTX64.EFI`).*
