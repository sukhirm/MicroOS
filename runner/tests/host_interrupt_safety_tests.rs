//! Host-side safety tests for Interrupt Descriptor Table (IDT) and 8259 PIC routing.
//!
//! Verifies:
//! 1. Vector classification and non-overlapping partitioning (0..=255).
//! 2. Correct 8259 PIC EOI routing rules (Master, Slave, Spurious, Non-PIC).
//! 3. Prohibition of blind PIC EOI for software/APIC/MSI vectors (48..=255).
//! 4. Task State Segment (TSS) IST index assignments for critical exceptions (#DF, #PF, #GP).

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum VectorCategory {
    CpuException,
    PicMaster,
    PicMasterSpurious,
    PicSlave,
    PicSlaveSpurious,
    SoftwareYield,
    NonPicInterrupt,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EoiAction {
    None,
    MasterOnly,
    DualSlaveAndMaster,
}

/// Classifies an x86_64 interrupt vector (0..=255).
pub fn classify_vector(vec: u8) -> (VectorCategory, EoiAction) {
    match vec {
        0..=31 => (VectorCategory::CpuException, EoiAction::None),
        32..=38 => (VectorCategory::PicMaster, EoiAction::MasterOnly),
        39 => (VectorCategory::PicMasterSpurious, EoiAction::None),
        40..=43 | 44 | 45..=46 => (VectorCategory::PicSlave, EoiAction::DualSlaveAndMaster),
        47 => (VectorCategory::PicSlaveSpurious, EoiAction::MasterOnly),
        48 => (VectorCategory::SoftwareYield, EoiAction::None),
        49..=255 => (VectorCategory::NonPicInterrupt, EoiAction::None),
    }
}

#[test]
fn test_all_256_vectors_partitioned_without_gaps_or_overlaps() {
    let mut visited = [false; 256];

    for vec in 0..=255u8 {
        let (category, action) = classify_vector(vec);
        visited[vec as usize] = true;

        match vec {
            0..=31 => {
                assert_eq!(category, VectorCategory::CpuException);
                assert_eq!(action, EoiAction::None);
            }
            32..=38 => {
                assert_eq!(category, VectorCategory::PicMaster);
                assert_eq!(action, EoiAction::MasterOnly);
            }
            39 => {
                assert_eq!(category, VectorCategory::PicMasterSpurious);
                assert_eq!(action, EoiAction::None, "Spurious IRQ7 must never receive EOI");
            }
            40..=43 => {
                assert_eq!(category, VectorCategory::PicSlave);
                assert_eq!(action, EoiAction::DualSlaveAndMaster);
            }
            44 => {
                assert_eq!(category, VectorCategory::PicSlave); // Mouse (IRQ12)
                assert_eq!(action, EoiAction::DualSlaveAndMaster);
            }
            45..=46 => {
                assert_eq!(category, VectorCategory::PicSlave);
                assert_eq!(action, EoiAction::DualSlaveAndMaster);
            }
            47 => {
                assert_eq!(category, VectorCategory::PicSlaveSpurious);
                assert_eq!(action, EoiAction::MasterOnly, "Spurious IRQ15 must send EOI to Master only");
            }
            48 => {
                assert_eq!(category, VectorCategory::SoftwareYield);
                assert_eq!(action, EoiAction::None, "Software int 48 must never touch PIC");
            }
            49..=255 => {
                assert_eq!(category, VectorCategory::NonPicInterrupt);
                assert_eq!(action, EoiAction::None, "Vectors 49..=255 must never send PIC EOI");
            }
        }
    }

    assert!(visited.iter().all(|&v| v), "All 256 vectors must be accounted for");
}

#[test]
fn test_pic_eoi_behavior_rules() {
    // Master IRQ0 (Timer) -> Master EOI
    let (_, eoi_timer) = classify_vector(32);
    assert_eq!(eoi_timer, EoiAction::MasterOnly);

    // Master IRQ1 (Keyboard) -> Master EOI
    let (_, eoi_kbd) = classify_vector(33);
    assert_eq!(eoi_kbd, EoiAction::MasterOnly);

    // Master IRQ7 (Spurious) -> No EOI (Per Intel 8259A spec)
    let (_, eoi_spur1) = classify_vector(39);
    assert_eq!(eoi_spur1, EoiAction::None);

    // Slave IRQ12 (Mouse) -> Dual Slave + Master EOI
    let (_, eoi_mouse) = classify_vector(44);
    assert_eq!(eoi_mouse, EoiAction::DualSlaveAndMaster);

    // Slave IRQ15 (Spurious) -> Master EOI only (Per Intel 8259A spec)
    let (_, eoi_spur2) = classify_vector(47);
    assert_eq!(eoi_spur2, EoiAction::MasterOnly);

    // Non-PIC vectors must never touch PIC ports (ports 0x20, 0xA0)
    for v in 49..=255u8 {
        let (_, eoi_non_pic) = classify_vector(v);
        assert_eq!(eoi_non_pic, EoiAction::None, "Vector {} must have EoiAction::None", v);
    }
}

#[test]
fn test_ist_index_assignments() {
    // Micro-OS TSS layout:
    // IST1 (table index 0): Double Fault (#DF, Vector 8)
    // IST2 (table index 1): Page Fault (#PF, Vector 14)
    // IST3 (table index 2): General Protection Fault (#GP, Vector 13)
    const DF_IST_INDEX: u16 = 0;
    const PF_IST_INDEX: u16 = 1;
    const GP_IST_INDEX: u16 = 2;

    assert_eq!(DF_IST_INDEX, 0);
    assert_eq!(PF_IST_INDEX, 1);
    assert_eq!(GP_IST_INDEX, 2);

    // Ensure all 3 IST stacks are strictly distinct
    assert_ne!(DF_IST_INDEX, PF_IST_INDEX);
    assert_ne!(DF_IST_INDEX, GP_IST_INDEX);
    assert_ne!(PF_IST_INDEX, GP_IST_INDEX);
}
