//! Source-derived regression cases for the repaired reference catalogues; no target IO.
use debugtui::registers::{Access, Catalogue, Component, Config as RegisterConfig, DescriptionLocation, Implementation, RawValue, Reader, Scope, Topology};
use debugtui::writes::{Input, InputKind, OnceState, Selection};
use serde_json::{Value, json};
use std::{fs, io::Write, path::{Path,PathBuf}, process::{Command,Stdio}, time::{SystemTime,UNIX_EPOCH}};

fn root() -> PathBuf { PathBuf::from(env!("CARGO_MANIFEST_DIR")) }
fn load(name:&str)->Catalogue { Catalogue::load(&root().join("reference").join(name)).unwrap() }

#[test]
fn r52_numproc_uses_original_low_three_and_high_two_bits_for_all_patterns() {
    let c=load("cortex-r52-system-registers.toml");
    let f=c.register("trcidr3").unwrap().fields.iter().find(|f|f.name=="NUMPROC").unwrap();
    // TRM Table15-75/76 PDF526–528: [30:28] is bits[2:0], [13:12] is bits[4:3].
    for value in 0u128..32 {
        let word=((value & 7)<<28)|((value>>3)<<12);
        assert_eq!(f.extract(&RawValue::from_integer(word,32).unwrap()).unwrap().integer().unwrap(),value);
    }
    assert!(c.registers.iter().all(|r|r.writer.is_none()));
}

#[test]
fn r52_all_mpu_views_preserve_permissions_and_include_source_trap_branches() {
    let c=load("cortex-r52-system-registers.toml");
    // R52 HSTR primary CRn6 and DDI0568 indirect views take precedence over
    // the conflicting T10 label in the supplement's direct region-view pages.
    for prefix in ["prbar","prlar","hprbar","hprlar"] {
        for index in std::iter::once(None).chain((0..24).map(Some)) {
            let id=match index {Some(n)=>format!("{prefix}{n}"),None=>prefix.to_owned()};
            let reg=c.register(&id).unwrap();assert_eq!(reg.access,Access::Rw);assert!(reg.writer.is_none());
            assert!(matches!(reg.reader,Reader::Cp15{crn:6,..}));assert!(reg.access_condition.contains("HSTR.T6"),"{id}");
            if prefix.starts_with('h') { assert!(reg.access_condition.contains("0时UNDEFINED"),"{id}"); }
            else { assert!(reg.access_condition.contains("HCR.TRVM"));assert!(reg.access_condition.contains("TVM")); }
        }
    }
    for id in ["prselr","hprselr"] { let reg=c.register(id).unwrap();assert!(reg.description.contains("ISB"));assert!(reg.description.contains("直接索引区域入口不依赖")); }
    for (id,crn,access) in [("hmpuir",0,Access::Ro),("hacr",1,Access::Rw),("hactlr2",1,Access::Rw),("hcr2",1,Access::Rw),("vsctlr",2,Access::Rw),("hprenr",6,Access::Rw),("hamair0",10,Access::Rw),("hamair1",10,Access::Rw),("icc_hsre",12,Access::Ro)] {
        let reg=c.register(id).unwrap();assert_eq!(reg.access,access);assert!(reg.access_condition.contains(&format!("HSTR.T{crn}")));assert!(reg.access_condition.contains("该trap不授予EL1正常读写许可"));
    }
    for id in ["icc_pmr","icv_pmr"] {assert!(c.register(id).unwrap().access_condition.contains("先检查HSTR.T12"));}
    let fpscr=c.register("fpscr").unwrap();assert!(fpscr.access_condition.contains("FPEXC.EN=1"));assert!(fpscr.access_condition.contains("非Hyp CPACR许可、FPEXC.EN、然后HCPTR"));
    let hcr=c.register("hcr").unwrap();let vm=hcr.fields.iter().find(|f|f.name=="VM").unwrap();assert!(vm.description.contains("必须为1"));
    let hactlr=c.register("hactlr").unwrap();let t=hactlr.fields.iter().find(|f|f.name=="TESTR1").unwrap();assert!(t.description.contains("公开TRM未说明操作参数或写入副作用"));assert!(!t.description.contains("整体 RAZ/WI 的实现行为仍保留"));
    let hprenr=c.register("hprenr").unwrap();for f in &hprenr.fields {assert!(f.description.contains("RAZ/WI"));}
}

#[test]
fn r52_etm_reserved_selectors_and_eight_comparator_filter_match_primary_layout() {
    let c=load("cortex-r52-system-registers.toml");
    // IHI0064H.b 7.3.78 PDF421: n=2*NUMACPAIRS. R52 has four pairs.
    let filter=c.register("trcvdsacctlr").unwrap();let exclude=filter.fields.iter().find(|f|f.name=="EXCLUDE").unwrap();
    assert_eq!((exclude.segments[0].offset,exclude.segments[0].width),(16,8));
    let reserved=filter.fields.iter().find(|f|f.name=="RESERVED_31_24").unwrap();assert_eq!((reserved.segments[0].offset,reserved.segments[0].width),(24,8));
    for byte in 0u128..256 {
        for top in [0u128,1,255] {
            let raw=RawValue::from_integer((byte<<16)|(top<<24),32).unwrap();
            assert_eq!(exclude.extract(&raw).unwrap().integer().unwrap(),byte);
            assert_eq!(reserved.extract(&raw).unwrap().integer().unwrap(),top);
        }
    }
    // 7.3.64 PDF407: only even lower registers implement PAIRINV[21].
    for index in 2..16 {
        let reg=c.register(&format!("trcrsctlr{index}")).unwrap();let f=reg.fields.iter().find(|f|f.name=="PAIRINV").unwrap();
        assert_eq!((f.segments[0].offset,f.segments[0].width),(21,1));
        if index%2==1 {assert_eq!(f.access,Some(Access::Ro));assert!(f.enum_name(&RawValue::from_integer(1,1).unwrap()).is_none());}
        else {assert_eq!(f.access.unwrap_or(reg.access),Access::Rw);assert!(f.enum_name(&RawValue::from_integer(1,1).unwrap()).is_some());}
    }
    // 7.3.1 PDF353: NUMCIDC<=1 and NUMVMIDC<=1 make all CONTEXT[6:4] RES0.
    for index in 0..8 {
        let reg=c.register(&format!("trcacatr{index}")).unwrap();let f=reg.fields.iter().find(|f|f.name=="CONTEXT").unwrap();
        assert_eq!((f.segments[0].offset,f.segments[0].width),(4,3));assert_eq!(f.access,Some(Access::Ro));
        for code in 1u128..8 {assert!(f.enum_name(&RawValue::from_integer(code,3).unwrap()).is_none());}
    }
    let mut selectors=vec![("trceventctl0r","SEL0","TYPE0"),("trceventctl0r","SEL1","TYPE1"),("trceventctl0r","SEL2","TYPE2"),("trceventctl0r","SEL3","TYPE3"),("trctsctlr","SEL","TYPE"),("trcvictlr","SEL","TYPE"),("trcvdctlr","SEL","TYPE"),("trcseqrstevr","RESETSEL","RESETTYPE")];
    for id in ["trcseqevr0","trcseqevr1","trcseqevr2"] {selectors.extend([(id,"B SEL","B TYPE"),(id,"F SEL","F TYPE")]);}
    for id in ["trccntctlr0","trccntctlr1"] {selectors.extend([(id,"RLDSEL","RLDTYPE"),(id,"CNTSEL","CNTTYPE")]);}
    assert_eq!(selectors.len(),18);
    for (id,selection,kind) in selectors {
        let reg=c.register(id).unwrap();let f=reg.fields.iter().find(|f|f.name==selection).unwrap();let t=reg.fields.iter().find(|f|f.name==kind).unwrap();
        assert_eq!(f.segments[0].width,4);assert_eq!(t.segments[0].width,1);
        for sel in 0u128..16 {for typ in 0u128..2 {let raw=RawValue::from_integer((sel<<f.segments[0].offset)|(typ<<t.segments[0].offset),32).unwrap();assert_eq!(f.extract(&raw).unwrap().integer().unwrap(),sel);assert_eq!(t.extract(&raw).unwrap().integer().unwrap(),typ);}}
        assert!(f.description.contains("TYPE=1且SEL=0为CONSTRAINED UNPREDICTABLE"));assert!(f.description.contains("仅1..7"));assert!(!f.description.contains("pair0是固定资源"));
    }
    assert!(c.register("trcpdsr").unwrap().read_side_effect);
    assert!(c.registers.iter().all(|r|r.writer.is_none()));
}

#[test]
fn r52_cti_identity_and_command_interfaces_keep_proven_values_and_read_guards() {
    let c=load("cortex-r52-system-registers.toml");let identity=c.register("ctidevarch").unwrap();
    // DDI0487 H9.5.17/.19 + R52 Table14-4 INOUT=01: CTIv2, revision remains unknown.
    let word=(0x23bu128<<21)|(1<<20)|0x1a14;
    for (name,offset,width,value) in [("ARCHITECT",21,11,0x23bu128),("PRESENT",20,1,1),("ARCHID",0,16,0x1a14)] {
        let f=identity.fields.iter().find(|f|f.name==name).unwrap();assert_eq!((f.segments[0].offset,f.segments[0].width),(offset,width));
        assert_eq!(f.access.unwrap_or(identity.access),Access::Ro);
        assert_eq!(f.extract(&RawValue::from_integer(word,32).unwrap()).unwrap().integer().unwrap(),value);
        assert!(f.enum_name(&RawValue::from_integer(value,width).unwrap()).is_some());
        assert!(f.enum_name(&RawValue::from_integer(0,width).unwrap()).is_none());
    }
    assert!(identity.reset.is_none());assert!(identity.fields.iter().find(|f|f.name=="REVISION").unwrap().description.contains("未给定R52固定revision"));
    for id in ["pmswinc","pmswinc_mmio","pmlar","ctiapppulse","ctiappclear"] {
        let reg=c.register(id).unwrap();assert_eq!(reg.access,Access::Wo);assert!(!reg.auto_read(Implementation::Yes));assert!(reg.writer.is_none());
    }
    let external=c.register("pmswinc_mmio").unwrap();assert!(!external.description.contains("架构执行的写精确计数"));assert!(!external.description.contains("两次有效写无需"));
    assert!(external.description.contains("是否为对应counter创建SW_INCR事件是CONSTRAINED UNPREDICTABLE"));
    for i in 0..4 {
        let name=format!("P{i}");let f=external.fields.iter().find(|f|f.name==name).unwrap();assert_eq!((f.segments[0].offset,f.segments[0].width),(i,1));assert!(f.description.contains("CONSTRAINED UNPREDICTABLE"));assert!(!f.description.contains("Warm reset UNKNOWN"));
        assert!(!c.register("pmswinc").unwrap().fields.iter().find(|f|f.name==name).unwrap().description.contains("Warm reset UNKNOWN"));
    }
    assert!(c.register("pmswinc").unwrap().description.contains("架构执行的写精确计数"));
    for id in ["ctichinstatus","ctichoutstatus"] {let reg=c.register(id).unwrap();assert_eq!(reg.access,Access::Ro);for f in reg.fields.iter().filter(|f|f.name.starts_with("CH")) {assert!(f.description.contains("debugger必须视为UNKNOWN"));}}
}

#[test]
fn r52_cache_control_encodings_and_product_limits_preserve_command_guards() {
    let c=load("cortex-r52-system-registers.toml");
    // ID_MMFR3 still reports architectural MaintBcst=2, with the product's local scope.
    let id=c.register("id_mmfr3").unwrap();let f=id.fields.iter().find(|f|f.name=="MaintBcst").unwrap();
    assert_eq!((f.segments[0].offset,f.segments[0].width),(12,4));
    assert_eq!(f.extract(&RawValue::from_integer(2<<12,32).unwrap()).unwrap().integer().unwrap(),2);
    assert!(f.enum_name(&RawValue::from_integer(2,4).unwrap()).unwrap().contains("R52仅当前核"));
    // MRRC/MCRR uses CRm15 and EC04, unlike the 32-bit primary-CRn traps.
    let cpu=c.register("cpuactlr").unwrap();assert_eq!(cpu.bits,64);assert!(matches!(cpu.reader,Reader::Cp15_64{cp:15,crm:15,..}));assert!(cpu.access_condition.contains("EC0x04"));
    for (id,bit) in [("csselr",0),("imp_bpctlr",9),("imp_csctlr",9)] {let r=c.register(id).unwrap();assert!(matches!(r.reader,Reader::Cp15{cp:15,crn,..} if crn==bit));assert!(r.access_condition.contains(&format!("HSTR.T{bit}")));assert!(r.access_condition.contains("EC0x03"));}
    for id in ["icialluis","bpiallis","iciallu","bpiall","icimvau","bpimva","dcimvac","dccmvac","dccmvau","dccimvac","dcisw","dccsw","dccisw","imp_cdbgdci"] {
        let reg=c.register(id).unwrap();assert_eq!(reg.access,Access::Wo);assert!(reg.writer.is_none());assert!(!reg.auto_read(Implementation::Yes));
    }
    let fi=c.register("sctlr").unwrap().fields.iter().find(|f|f.name=="FI").unwrap();assert_eq!(fi.access,Some(Access::Ro));
    assert!(c.register("imp_csctlr").unwrap().description.contains("随后即使两者均关闭也不能重新划分"));
    assert!(c.register("ccsidr").unwrap().description.contains("不能作为有效几何或假设为零"));
}

#[test]
fn r52_fault_reset_and_signal_descriptors_preserve_unknowns_and_address_encoding() {
    let c=load("cortex-r52-system-registers.toml");
    // TRM Table4-16 gives zero for these four records, not all error registers.
    for id in ["imp_dcerr0","imp_dcerr1","imp_flasherr0","imp_flasherr1"] {let reg=c.register(id).unwrap();assert_eq!(reg.reset,Some(0));assert!(!reg.read_side_effect);assert!(reg.writer.is_none());}
    for id in ["imp_atcmregionr","imp_btcmregionr","imp_ctcmregionr"] {
        let reg=c.register(id).unwrap();assert!(reg.reset.is_none());let f=reg.fields.iter().find(|f|f.name=="BASEADDRESS").unwrap();
        assert_eq!((f.segments[0].offset,f.segments[0].width),(13,19));assert!(f.description.contains("仍待官方消歧"));assert!(!f.description.contains("按专门寄存器表保留0"));
    }
    let hpfar=c.register("hpfar").unwrap();let f=hpfar.fields.iter().find(|f|f.name=="FIPA[39:12]").unwrap();assert_eq!((f.segments[0].offset,f.segments[0].width),(4,28));
    for page in [0u128,1,0x1234567,0xfffffff] {assert_eq!(f.extract(&RawValue::from_integer(page<<4,32).unwrap()).unwrap().integer().unwrap(),page);}
    let test=c.register("imp_testr0").unwrap();assert_eq!(test.access,Access::Ro);
    for name in ["VIRQ","IRQ","VFIQ","FIQ"] {
        let f=test.fields.iter().find(|f|f.name==name).unwrap();assert_eq!(f.segments[0].width,1);
        for raw in 0u128..2 {let value=f.extract(&RawValue::from_integer(raw<<f.segments[0].offset,32).unwrap()).unwrap();assert_eq!(value.integer().unwrap(),raw);assert!(f.enum_name(&value).unwrap().contains("assertion极性未证"));}
        assert!(!f.description.contains("按1断言/0未断言"));
    }
    for (id,bit) in [("dfar",6),("dfsr",5),("hadfsr",5),("haifsr",5)] {assert!(c.register(id).unwrap().access_condition.contains(&format!("HSTR.T{bit}")));}
    for id in ["imp_tcmsyndr0","imp_tcmsyndr1"] {let reg=c.register(id).unwrap();assert_eq!(reg.access,Access::Ro);assert!(reg.writer.is_none());}
    assert!(c.register("hsr").unwrap().description.contains("不生成本次IRQ/FIQ的新syndrome"));
}

#[test]
fn r52_virtual_gic_priority_slots_and_command_directions_match_source_rules() {
    let c=load("cortex-r52-system-registers.toml");
    // IHI0069H.b 6.3.2: APR tracks acknowledged priorities not yet dropped.
    for id in ["icv_ap0r0","icv_ap1r0","ich_ap0r0","ich_ap1r0"] {
        let reg=c.register(id).unwrap();assert_eq!(reg.fields.len(),32);
        for slot in 0..32 {
            let name=format!("P{slot}");let f=reg.fields.iter().find(|f|f.name==name).unwrap();assert_eq!((f.segments[0].offset,f.segments[0].width),(slot,1));
            assert_eq!(f.extract(&RawValue::from_integer(1u128<<slot,32).unwrap()).unwrap().integer().unwrap(),1);
            assert!(f.description.contains("尚未priority drop"));assert!(f.description.contains("所有active均已drop"));
        }
    }
    for id in ["icv_rpr","icv_iar0","icv_iar1","icv_hppir0","icv_hppir1"] {let reg=c.register(id).unwrap();assert_eq!(reg.access,Access::Ro);assert!(!reg.description.contains("架构入口MRC/MCR"));assert!(reg.writer.is_none());}
    for id in ["icv_eoir0","icv_eoir1","icv_dir"] {let reg=c.register(id).unwrap();assert_eq!(reg.access,Access::Wo);assert!(!reg.auto_read(Implementation::Yes));assert!(!reg.description.contains("架构入口MRC/MCR"));assert!(!reg.description.contains("此视图时可读"));assert!(reg.writer.is_none());}
    for index in 0..4 {
        let reg=c.register(&format!("ich_lrc{index}")).unwrap();let f=reg.fields.iter().find(|f|f.name=="State").unwrap();assert_eq!((f.segments[0].offset,f.segments[0].width),(30,2));
        for state in 0u128..4 {assert_eq!(f.extract(&RawValue::from_integer(state<<30,32).unwrap()).unwrap().integer().unwrap(),state);assert!(f.enum_name(&RawValue::from_integer(state,2).unwrap()).is_some());}
        assert!(f.enum_name(&RawValue::from_integer(3,2).unwrap()).unwrap().contains("仅HW=0"));
    }
    let hsre=c.register("icc_hsre").unwrap();assert_eq!(hsre.access,Access::Ro);assert_eq!(hsre.reset,Some(15));assert!(hsre.writer.is_none());assert!(hsre.description.contains("正常EL2允许相应MCR访问"));
    assert!(c.register("ich_elrsr").unwrap().fields.iter().find(|f|f.name=="Status<n>").unwrap().description.contains("State00/HW0/EOI1时EISR1且ELRSR0"));
}

#[test]
fn r52_saved_state_and_debug_protocol_descriptors_keep_register_roles_distinct() {
    let c=load("cortex-r52-system-registers.toml");
    // SPSR banks preserve five-bit mode encodings, but EL1 cannot return to EL2.
    for id in ["spsr_fiq","spsr_irq","spsr_svc","spsr_abt","spsr_und"] {
        let reg=c.register(id).unwrap();let f=reg.fields.iter().find(|f|f.name=="M").unwrap();assert_eq!((f.segments[0].offset,f.segments[0].width),(0,5));
        assert_eq!(f.extract(&RawValue::from_integer(0x1a,32).unwrap()).unwrap().integer().unwrap(),0x1a);
        assert!(f.enum_name(&RawValue::from_integer(0x1a,5).unwrap()).unwrap().contains("此EL1返回上下文不合法"));
        for field in ["A","C","E","F","GE","I","IL","IT","M","N","Q","T","V","Z"] {assert!(reg.fields.iter().find(|f|f.name==field).unwrap().description.contains("MSR SPSR_<bank>,Rn更新整32位"));}
    }
    let dspsr=c.register("dspsr").unwrap();for field in ["A","C","E","F","GE","I","IL","IT","M","N","Q","T","V","Z"] {assert!(dspsr.fields.iter().find(|f|f.name==field).unwrap().description.contains("DSPSR通过MCR更新整字"));}
    let dlr=c.register("dlr").unwrap();assert!(dlr.reset.is_none());assert!(dlr.fields.iter().find(|f|f.name=="RESTART_ADDR").unwrap().description.contains("包括reset catch场景"));
    let edscr=c.register("edscr").unwrap();assert!(edscr.reset.is_none());
    let ite=edscr.fields.iter().find(|f|f.name=="ITE").unwrap();assert!(!ite.description.contains("执行已完成状态"));assert!(ite.description.contains("允许已有指令仍未完成"));
    assert!(edscr.fields.iter().find(|f|f.name=="STATUS").unwrap().description.contains("Core/Cold reset值0b000010"));
    for (id,access) in [("edlar",Access::Wo),("dbgoslar_mmio",Access::Wo),("dbgauthstatus_el1",Access::Ro),("eddevaff0",Access::Ro)] {
        let reg=c.register(id).unwrap();assert_eq!(reg.access,access);assert!(reg.writer.is_none());assert!(!reg.access_condition.contains("没有这些外部身份字的普通CP14/CP15入口"));
        if access==Access::Wo {assert!(!reg.auto_read(Implementation::Yes));}
    }
    assert!(c.register("dbgoslar_mmio").unwrap().access_condition.contains("写返回ERROR"));
    assert!(c.register("edprsr").unwrap().read_side_effect);assert!(c.register("editr").unwrap().writer.is_none());
}

#[test]
fn r52_rom_cluster_ownership_and_breakpoint_readonly_mirrors_are_explicit() {
    let c=load("cortex-r52-system-registers.toml");let mut rom_count=0;
    for reg in &c.registers {
        if let Reader::Mmio{component,require_owner_mapping,..}=&reg.reader && component=="rom" {
            rom_count+=1;assert_eq!(reg.scope,Scope::Cluster);assert!(*require_owner_mapping);assert!(reg.writer.is_none());
        }
    }
    assert_eq!(rom_count,28);
    let mut topology=Topology::default();assert!(topology.owner(Scope::Cluster,"synthetic-core-a").is_none());
    topology.clusters.insert("synthetic-core-a".into(),"processor-a".into());topology.clusters.insert("synthetic-core-b".into(),"processor-a".into());topology.validate().unwrap();
    assert_eq!(topology.owner(Scope::Cluster,"synthetic-core-a"),Some("cluster:processor-a".into()));assert_eq!(topology.owner(Scope::Cluster,"synthetic-core-b"),Some("cluster:processor-a".into()));
    // Synthetic routing data only: a global or another core's base cannot stand in for the cluster.
    let mut config=RegisterConfig::default();config.components.insert("rom".into(),Component{base:0,channel:"synthetic".into(),little_endian:true});
    assert!(config.component("rom",Some("cluster:processor-a"),true).is_err());
    config.component_owners.insert("rom".into(),[("core:synthetic-core-a".into(),Component{base:0,channel:"synthetic".into(),little_endian:true})].into_iter().collect());
    assert!(config.component("rom",Some("cluster:processor-a"),true).is_err());
    config.component_owners.get_mut("rom").unwrap().insert("cluster:processor-a".into(),Component{base:0,channel:"synthetic".into(),little_endian:true});
    assert!(config.component("rom",Some("cluster:processor-a"),true).is_ok());assert!(config.component("rom",None,true).is_err());
    for index in 0..8 {
        for suffix in ["","_mmio"] {
            let reg=c.register(&format!("dbgbcr{index}{suffix}")).unwrap();let select=reg.fields.iter().find(|f|f.name=="BAS").unwrap();let mirror=reg.fields.iter().find(|f|f.name=="BAS_MIRROR").unwrap();
            assert_eq!(select.access,Some(Access::Rw));assert_eq!(mirror.access,Some(Access::Ro));
            assert_eq!(select.segments.iter().map(|s|(s.offset,s.width)).collect::<Vec<_>>(),vec![(5,1),(7,1)]);assert_eq!(mirror.segments.iter().map(|s|(s.offset,s.width)).collect::<Vec<_>>(),vec![(6,1),(8,1)]);
            // TRM C.6: raw BAS0000/0011/1100/1111 have mirrored adjacent bit pairs.
            for (raw,logical) in [(0u128,0),(3,1),(12,2),(15,3)] {let value=RawValue::from_integer(raw<<5,32).unwrap();assert_eq!(select.extract(&value).unwrap().integer().unwrap(),logical);assert_eq!(mirror.extract(&value).unwrap().integer().unwrap(),logical);assert!(select.enum_name(&RawValue::from_integer(logical,2).unwrap()).is_some());}
            assert!(reg.writer.is_none());
        }
    }
}

#[test]
fn f429_missing_transfer_interfaces_have_real_locations_without_command_readers() {
    let c=load("stm32f429-system-registers.toml");
    for (id,address) in [("nvic.stir",0xe000ef00),("dcb.dcrsr",0xe000edf4)] {
        let reg=c.register(id).unwrap();assert_eq!(reg.access,Access::Wo);assert!(matches!(reg.reader,Reader::DescriptionOnly{location:DescriptionLocation::CorePrivate{address:a}} if a==address));assert!(!reg.auto_read(Implementation::Yes));assert!(reg.writer.is_none() && reg.write.is_none());
        let mut bad=c.clone();let r=bad.registers.iter_mut().find(|r|r.id==id).unwrap();r.access=Access::Rw;assert!(bad.validate().unwrap_err().contains("Description-only"));
        let mut bad=c.clone();bad.registers.iter_mut().find(|r|r.id==id).unwrap().reader=Reader::DescriptionOnly{location:DescriptionLocation::CorePrivate{address:address+1}};assert!(bad.validate().is_err());
        let mut bad=c.clone();bad.registers.iter_mut().find(|r|r.id==id).unwrap().read_side_effect=true;assert!(bad.validate().is_err());
    }
    let reg=c.register("dcb.dcrsr").unwrap();let selector=reg.fields.iter().find(|f|f.name=="REGSEL").unwrap();assert_eq!((selector.segments[0].offset,selector.segments[0].width),(0,7));
    for (code,name) in [(0,"R0"),(18,"PSP"),(20,"CONTROL:FAULTMASK:BASEPRI:PRIMASK"),(33,"FPSCR"),(64,"S0"),(95,"S31")] {assert_eq!(selector.enum_name(&RawValue::from_integer(code,7).unwrap()),Some(name));}
    for code in [19,21,32,34,63,96,127] {assert!(selector.enum_name(&RawValue::from_integer(code,7).unwrap()).is_none());}
    assert_eq!(reg.fields.iter().find(|f|f.name=="REGWnR").unwrap().segments[0].offset,16);
    let data=c.register("dcb.dcrdr").unwrap();assert_eq!(data.access,Access::Rw);assert!(matches!(data.reader,Reader::CorePrivate{address:0xe000edf8}));assert!(data.reset.is_none() && data.writer.is_none());assert_eq!(data.fields[0].segments[0].width,32);
    for cpu in ["cortex-m3","cortex-m4","cortex-m7"] {let catalogue=Catalogue::builtin(cpu).unwrap();catalogue.validate().unwrap();assert!(catalogue.registers.iter().all(|r|!matches!(r.reader,Reader::DescriptionOnly{..})));}
}

#[test]
fn f429_dbgmcu_and_formatter_match_primary_geometry_permissions_and_reset_domains() {
    let c=load("stm32f429-system-registers.toml");
    for (id,offset,access) in [("dbgmcu.idcode",0,Access::Ro),("dbgmcu.cr",4,Access::Rw),("dbgmcu.apb1_fz",8,Access::Rw),("dbgmcu.apb2_fz",12,Access::Rw)] {
        let reg=c.register(id).unwrap();assert_eq!(reg.scope,Scope::Chip);assert_eq!(reg.access,access);assert_eq!(reg.bits,32);assert!(matches!(&reg.reader,Reader::Mmio{component,offset:o,require_owner_mapping:true} if component=="dbgmcu" && *o==offset));assert!(reg.writer.is_none());
        if access==Access::Rw {assert_eq!(reg.reset,Some(0));assert!(reg.description.contains("SYSRESET不清除"));} else {assert!(reg.reset.is_none());}
    }
    let id=c.register("dbgmcu.idcode").unwrap();let dev=id.fields.iter().find(|f|f.name=="DEV_ID").unwrap();assert_eq!((dev.segments[0].offset,dev.segments[0].width),(0,12));assert!(dev.enum_name(&RawValue::from_integer(0x419,12).unwrap()).is_some());assert!(dev.enum_name(&RawValue::from_integer(0x413,12).unwrap()).is_none());
    let apb2=c.register("dbgmcu.apb2_fz").unwrap();for (name,bit) in [("DBG_TIM1_STOP",0),("DBG_TIM8_STOP",1),("DBG_TIM9_STOP",16),("DBG_TIM10_STOP",17),("DBG_TIM11_STOP",18)] {let f=apb2.fields.iter().find(|f|f.name==name).unwrap();assert_eq!((f.segments[0].offset,f.segments[0].width),(bit,1));}
    let ffsr=c.register("tpiu.ffsr").unwrap();assert_eq!(ffsr.access,Access::Ro);assert_eq!(ffsr.reset,Some(8));assert!(matches!(ffsr.reader,Reader::CorePrivate{address:0xe0040300}));
    for (name,bit,value) in [("FtNonStop",3,1),("TCPresent",2,0),("FtStopped",1,0),("FlInProg",0,0)] {let f=ffsr.fields.iter().find(|f|f.name==name).unwrap();assert_eq!(f.segments[0].offset,bit);assert_eq!(f.extract(&RawValue::from_integer(8,32).unwrap()).unwrap().integer().unwrap(),value);}
    let ffcr=c.register("tpiu.ffcr").unwrap();assert_eq!(ffcr.access,Access::Rw);assert_eq!(ffcr.reset,Some(0x102));assert!(matches!(ffcr.reader,Reader::CorePrivate{address:0xe0040304}));
    let mut writable=0u128;for f in &ffcr.fields {if f.access==Some(Access::Rw) {for s in &f.segments {writable|=((1u128<<s.width)-1)<<s.offset;}}}assert_eq!(writable,2);assert_eq!(ffcr.fields.iter().find(|f|f.name=="TrigIn").unwrap().access,Some(Access::Ro));
    for reg in [ffsr,ffcr] {assert!(reg.access_rule.need_enable.is_some());assert!(reg.description.contains("两HCLK周期"));assert!(reg.writer.is_none());}
}

#[test]
fn f429_actlr_r0p1_and_all_priority_byte_views_match_independent_source_positions() {
    let c=load("stm32f429-system-registers.toml");let act=c.register("scb.actlr").unwrap();
    // 100166_0001_04_en r0p1 Table4-2 PDF53, not the swapped older r0p0 figure.
    for (name,bit) in [("DISOOFP",9),("DISFPCA",8),("DISFOLD",2),("DISDEFWBUF",1),("DISMCYCINT",0)] {
        let f=act.fields.iter().find(|f|f.name==name).unwrap();
        assert_eq!(f.segments.len(),1);assert_eq!(f.segments[0].offset,bit);assert_eq!(f.segments[0].width,1);
        assert_eq!(f.extract(&RawValue::from_integer(1u128<<bit,32).unwrap()).unwrap().integer().unwrap(),1);
    }
    let dis=act.fields.iter().find(|f|f.name=="DISMCYCINT").unwrap();assert!(dis.description.contains("增加中断延迟"));
    let oofp=act.fields.iter().find(|f|f.name=="DISOOFP").unwrap();assert_eq!(oofp.access,Some(Access::Rw));
    let fpca=act.fields.iter().find(|f|f.name=="DISFPCA").unwrap();assert!(fpca.description.contains("写零或保持"));
    assert!(!act.fields_missing);
    // PM0214 Figure19/Table47 and F429 implementation declaration: 4 MSBs per byte.
    for index in 0..240 {
        let reg=c.register(&format!("nvic.ipr{index}")).unwrap();assert_eq!(reg.bits,8);
        if [79,91,92,93,94,95].contains(&index) {
            assert_eq!(reg.access,Access::Ro);
            assert_eq!(reg.fields.len(),1);
            let f=&reg.fields[0];assert_eq!(f.name,"RESERVED");assert_eq!(f.access,Some(Access::Ro));
            assert_eq!((f.segments[0].offset,f.segments[0].width),(0,8));
            assert!(reg.writer.is_none());continue;
        }
        let f=reg.fields.iter().find(|f|f.name=="PRIORITY").unwrap();
        for raw in 0u128..256 { assert_eq!(f.extract(&RawValue::from_integer(raw,8).unwrap()).unwrap().integer().unwrap(),raw>>4); }
        assert_eq!(reg.fields.iter().find(|f|f.name=="RESERVED_3_0").unwrap().access,Some(Access::Ro));
        assert!(reg.writer.is_none());
    }
    for bank in 0..8 {
        for (family,name,access) in [("iser","SETENA",Access::Rw),("icer","CLRENA",Access::Rw),("ispr","SETPEND",Access::Rw),("icpr","CLRPEND",Access::Rw),("iabr","ACTIVE",Access::Ro)] {
            let reg=c.register(&format!("nvic.{family}{bank}")).unwrap();assert_eq!(reg.access,access);
            if bank==2 {
                let mut writable_mask=0u128;
                for (field_name,offset,width,field_access) in [(format!("{name}_64_78"),0,15,access),("RESERVED_IRQ79".into(),15,1,Access::Ro),(format!("{name}_80_90"),16,11,access),("RESERVED_IRQ91_95".into(),27,5,Access::Ro)] {
                    let f=reg.fields.iter().find(|f|f.name==field_name).unwrap();
                    assert_eq!((f.segments[0].offset,f.segments[0].width),(offset,width));assert_eq!(f.access,Some(field_access));
                    if field_access==Access::Rw { writable_mask|=((1u128<<width)-1)<<offset; }
                }
                assert_eq!(writable_mask,if access==Access::Rw {0x07ff7fff} else {0});
                assert_eq!(reg.fields.len(),4);
            } else {
                let f=reg.fields.iter().find(|f|f.name==name).unwrap();assert_eq!(f.segments[0].offset,0);assert_eq!(f.segments[0].width,32);
            }
            assert!(reg.writer.is_none() && !reg.read_side_effect);
        }
    }
    // Frozen pre-repair catalogue: exactly R0..R12/SP/LR/PC have integer writers.
    let mut writers:Vec<_>=c.registers.iter().filter(|r|r.writer.is_some()).map(|r|r.id.as_str()).collect();writers.sort_unstable();
    let mut expected:Vec<_>=(0..13).map(|i|format!("r{i}")).chain(["sp","lr","pc"].into_iter().map(str::to_owned)).collect();expected.sort_unstable();assert_eq!(writers,expected);
    for r in &c.registers { assert!(r.source.is_some(),"{}",r.id); }
    for i in 0..4 { assert!(c.register(&format!("dwt.function{i}")).unwrap().read_side_effect); }
}

#[test]
fn f429_mpu_aliases_optional_components_and_existing_address_write_policies() {
    let c=load("stm32f429-system-registers.toml");
    // PM0214 MPU register map: these are actual alternate address windows, not bank numbers.
    for (id,address) in [("mpu.rbar_a1",0xe000eda4),("mpu.rasr_a1",0xe000eda8),("mpu.rbar_a2",0xe000edac),("mpu.rasr_a2",0xe000edb0),("mpu.rbar_a3",0xe000edb4),("mpu.rasr_a3",0xe000edb8)] {
        let reg=c.register(id).unwrap();assert!(matches!(reg.reader,Reader::CorePrivate{address:a} if a==address));
        assert!(reg.writer.is_none());assert!(!reg.fields_missing);
    }
    let ap=c.register("mpu.rasr").unwrap().fields.iter().find(|f|f.name=="AP").unwrap();
    for (code,label) in [(4,"保留；UNPREDICTABLE"),(7,"特权/非特权均RO")] {
        assert_eq!(ap.enum_name(&RawValue::from_integer(code,3).unwrap()),Some(label));
    }
    let size=c.register("mpu.rasr").unwrap().fields.iter().find(|f|f.name=="SIZE").unwrap();
    assert!(size.enum_name(&RawValue::from_integer(3,5).unwrap()).unwrap().contains("保留"));
    for code in 4..32 { assert_eq!(size.enum_name(&RawValue::from_integer(code,5).unwrap()).unwrap(),format!("{}字节区域",1u64<<(code+1))); }
    for (id,fact) in [("fpu.mvfr2","stm32f429.mvfr2.present"),("dwt.pcsr","stm32f429.dwt.pcsr.present")] {
        let reg=c.register(id).unwrap();let (state,reason)=reg.implementation(&Default::default());assert_eq!(state,Implementation::Unknown);assert!(reason.contains(fact));
        let facts=[(fact.to_owned(),0)].into_iter().collect();assert_eq!(reg.implementation(&facts).0,Implementation::No);
    }
    // Pure write planning only. Reserved low address bits become zero; no target writes.
    for (id,offset) in [("sp",2),("pc",1)] {
        let reg=c.register(id).unwrap();let spec=reg.write.as_ref().unwrap();
        for raw in [0u128,1,2,3,0x2000_0003,0xffff_ffff] {
            let plan=spec.prepare(Selection::Register,&Input{kind:InputKind::Unsigned,text:raw.to_string(),little_endian:None},OnceState::Unknown).unwrap();
            let mask=0xffff_ffffu128 & !((1<<offset)-1);
            assert_eq!(plan.selected_mask.integer().unwrap(),mask);assert_eq!(plan.verification_mask.integer().unwrap(),mask);
            assert!(!plan.needs_fresh_read);assert_eq!(plan.resolve(None).unwrap().command.integer().unwrap(),raw & mask);
        }
        assert!(spec.prepare(Selection::Field{name:"RESERVED_LOW".into()},&Input{kind:InputKind::Unsigned,text:"1".into(),little_endian:None},OnceState::Unknown).is_err());
        assert!(spec.prepare(Selection::Register,&Input{kind:InputKind::Unsigned,text:"4294967296".into(),little_endian:None},OnceState::Unknown).is_err());
    }
}

#[test]
fn f429_identity_word_views_and_signed_rom_entries_keep_presence_and_unknown_bits() {
    let c=load("stm32f429-system-registers.toml");
    // Cortex-M4 r0p1 Table8-3, Armv7-M D1.1: conceptual byte IDs use word reads.
    for (prefix,base) in [("scs",0xe000e000),("fpb",0xe0002000),("dwt",0xe0001000),("rom",0xe00ff000)] {
        for (suffix,offset) in [("pid4",0xfd0),("pid0",0xfe0),("pid1",0xfe4),("pid2",0xfe8),("pid3",0xfec),("cid0",0xff0),("cid1",0xff4),("cid2",0xff8),("cid3",0xffc)] {
            let reg=c.register(&format!("{prefix}.{suffix}")).unwrap();assert_eq!(reg.bits,32);assert_eq!(reg.access,Access::Ro);assert!(reg.writer.is_none() && reg.reset.is_none());assert!(matches!(reg.reader,Reader::CorePrivate{address:a} if a==base+offset));
            let upper=reg.fields.iter().find(|f|f.name=="ZeroExtendedUpper").unwrap();assert_eq!((upper.segments[0].offset,upper.segments[0].width),(8,24));assert_eq!(upper.access,Some(Access::Ro));
        }
    }
    for index in 5..8 {
        let reg=c.register(&format!("scs.pid{index}")).unwrap();let fact=format!("stm32f429.scs.pid{index}.present");assert_eq!(reg.implementation(&Default::default()).0,Implementation::Unknown);
        for (v,state) in [(0,Implementation::No),(1,Implementation::Yes),(2,Implementation::No)] {let facts=[(fact.clone(),v)].into_iter().collect();assert_eq!(reg.implementation(&facts).0,state);}
    }
    // Raw product ROM words, expected absolute addresses from the primary component table.
    for (suffix,offset,word,expected) in [("scs",0,0xfff0f003u32,0xe000e000u32),("dwt",4,0xfff02003,0xe0001000),("fpb",8,0xfff03003,0xe0002000),("itm",12,0xfff01003,0xe0000000),("tpiu",16,0xfff41003,0xe0040000),("etm",20,0xfff42003,0xe0041000)] {
        let reg=c.register(&format!("rom.{suffix}")).unwrap();assert!(matches!(reg.reader,Reader::CorePrivate{address:a} if a==0xe00ff000+offset));
        let raw=RawValue::from_integer(word as u128,32).unwrap();let address=reg.fields.iter().find(|f|f.name=="ADDRESSOFFSET").unwrap().extract(&raw).unwrap().integer().unwrap() as i64;
        let signed=(address^0x80000)-0x80000;assert_eq!((0xe00ff000i64+(signed<<12)) as u32,expected);
        assert_eq!(reg.fields.iter().find(|f|f.name=="FORMAT").unwrap().extract(&raw).unwrap().integer().unwrap(),1);
        assert_eq!(reg.fields.iter().find(|f|f.name=="PRESENT").unwrap().extract(&RawValue::from_integer((word & !1) as u128,32).unwrap()).unwrap().integer().unwrap(),0);
        assert!(reg.description.contains("非零") && reg.description.contains("wholeword0"));
    }
    let mem=c.register("rom.memtype").unwrap();assert!(matches!(mem.reader,Reader::CorePrivate{address:0xe00fffcc}));let unknown=mem.fields.iter().find(|f|f.name=="UNKNOWN").unwrap();assert_eq!((unknown.segments[0].offset,unknown.segments[0].width),(1,31));assert!(unknown.description.contains("UNKNOWN") && unknown.enums.is_empty());assert!(mem.reset.is_none());
}

#[test]
fn itm_stimulus_ports_separate_read_status_and_descriptive_write_payloads() {
    let c=load("stm32f429-system-registers.toml");
    for index in 0..32 {
        let reg=c.register(&format!("itm.stim{index}")).unwrap();assert!(matches!(reg.reader,Reader::CorePrivate{address:a} if a==0xe0000000+4*index));assert_eq!(reg.access,Access::Rw);assert!(reg.writer.is_none() && reg.write.is_none());assert_eq!(reg.fields.len(),2);assert_eq!(reg.write_fields.len(),1);
        let ready=reg.fields.iter().find(|f|f.name=="FIFOREADY").unwrap();assert_eq!(ready.access,Some(Access::Ro));assert_eq!((ready.segments[0].offset,ready.segments[0].width),(0,1));
        let payload=&reg.write_fields[0];assert_eq!(payload.name,"STIMULUS");assert_eq!(payload.access,Some(Access::Wo));assert_eq!((payload.segments[0].offset,payload.segments[0].width),(0,32));assert!(!reg.fields.iter().any(|f|f.name=="STIMULUS"));
    }
    assert!(c.register("itm.stim32").is_none());let ter=c.register("itm.ter").unwrap();assert_eq!(ter.fields.len(),32);
    for index in 0..32 {let f=ter.fields.iter().find(|f|f.name==format!("STIMENA{index}")).unwrap();assert_eq!((f.segments[0].offset,f.segments[0].width),(index,1));}
    let mut bad=c.clone();bad.registers.iter_mut().find(|r|r.id=="itm.stim0").unwrap().write_fields[0].access=Some(Access::Ro);assert!(bad.validate().unwrap_err().contains("descriptive write field"));
    let mut bad=c.clone();let r=bad.registers.iter_mut().find(|r|r.id=="itm.stim0").unwrap();let mut duplicate=r.write_fields[0].clone();duplicate.name="SECOND_PAYLOAD".into();r.write_fields.push(duplicate);assert!(bad.validate().unwrap_err().contains("Overlapping descriptive"));
    let mut bad=c.clone();bad.registers.iter_mut().find(|r|r.id=="itm.stim0").unwrap().write_fields[0].segments[0].width=33;assert!(bad.validate().is_err());
    let mut bad=c.clone();bad.registers.iter_mut().find(|r|r.id=="itm.stim0").unwrap().access=Access::Ro;assert!(bad.validate().unwrap_err().contains("Descriptive write fields"));
    let mut bad=c.clone();let original=c.register("r0").unwrap();let r=bad.registers.iter_mut().find(|r|r.id=="itm.stim0").unwrap();r.writer=original.writer.clone();r.write=original.write.clone();assert!(bad.validate().is_err());
    for cpu in ["cortex-m3","cortex-m4","cortex-m7"] {let catalogue=Catalogue::builtin(cpu).unwrap();catalogue.validate().unwrap();assert!(catalogue.registers.iter().all(|r|r.write_fields.is_empty() && !matches!(r.reader,Reader::DescriptionUnsupported{..})));}
}

#[test]
fn tpiu_target_geometry_and_etm_m4_capabilities_preserve_interface_exceptions() {
    let c=load("stm32f429-system-registers.toml");assert!(c.register("tpiu.fscr").is_none());
    let acpr=c.register("tpiu.acpr").unwrap();let divisor=acpr.fields.iter().find(|f|f.name=="PRESCALER").unwrap();assert_eq!((divisor.segments[0].offset,divisor.segments[0].width),(0,13));assert_eq!(divisor.extract(&RawValue::from_integer(0xffffffff,32).unwrap()).unwrap().integer().unwrap(),8191);assert_eq!(acpr.fields.iter().find(|f|f.name=="RESERVED_31_13").unwrap().access,Some(Access::Ro));
    let type_reg=c.register("tpiu.devid").unwrap();for (name,bit) in [("NRZ",11),("MANCHESTER",10),("PTINVALID",9),("ASYNC_TRACECLKIN",5)] {let f=type_reg.fields.iter().find(|f|f.name==name).unwrap();assert_eq!((f.segments[0].offset,f.segments[0].width),(bit,1));assert_eq!(f.access,Some(Access::Ro));}
    assert!(c.register("tpiu.type").is_none());assert!(type_reg.description.contains("TYPE/DEVID同址"));
    for id in ["tpiu.fifo0","tpiu.fifo1"] {let r=c.register(id).unwrap();assert_eq!(r.access,Access::Ro);assert!(r.read_side_effect && !r.auto_read(Implementation::Yes));assert!(r.reset.is_none());let gate=r.access_rule.need_enable.as_ref().unwrap();assert_eq!(gate.reg,"tpiu.itctrl");assert_eq!(gate.field,"MODE");assert_eq!(gate.value,2);}
    for (id,address) in [("etm.ittrigout",0xe0041ee8),("etm.itatbctr0",0xe0041ef8)] {let r=c.register(id).unwrap();assert_eq!(r.access,Access::Wo);assert!(matches!(r.reader,Reader::DescriptionOnly{location:DescriptionLocation::CorePrivate{address:a}} if a==address));assert!(r.writer.is_none());}
    let lar=c.register("etm.lar").unwrap();assert_eq!(lar.access,Access::Rw);assert!(matches!(lar.reader,Reader::DescriptionUnsupported{location:DescriptionLocation::CorePrivate{address:0xe0041fb0}}));assert!(lar.writer.is_none() && !lar.auto_read(Implementation::Yes));assert!(lar.reset.is_none());
    let ccer=c.register("etm.ccer").unwrap();assert_eq!(ccer.fields.iter().find(|f|f.name=="NoDataAddressComparisons").unwrap().segments[0].offset,12);assert_eq!(ccer.fields.iter().find(|f|f.name=="RegistersReadable").unwrap().segments[0].offset,11);
    assert_eq!(c.register("etm.syncfr").unwrap().access,Access::Ro);assert_eq!(c.register("etm.syncfr").unwrap().reset,Some(1024));assert_eq!(c.register("etm.pdsr").unwrap().reset,Some(1));
    let counter=c.register("etm.cntrldvr1").unwrap();assert_eq!(counter.fields.iter().find(|f|f.name=="ReloadValue").unwrap().segments[0].width,16);assert!(c.register("etm.cntrldvr2").is_none());
    let fflr=c.register("etm.fflr").unwrap();assert!(matches!(fflr.reader,Reader::CorePrivate{address:0xe0041028}));let threshold=fflr.fields.iter().find(|f|f.name=="FreeBytesThreshold").unwrap();assert_eq!(threshold.segments[0].width,8);assert!(threshold.description.contains("不是已占用字节数"));
    for id in ["etm.trigger","etm.teevr","etm.tsevr"] {let r=c.register(id).unwrap();for (code,expected) in ["A","NOT(A)","A AND B","NOT(A) AND B","NOT(A) AND NOT(B)","A OR B","NOT(A) OR B","NOT(A) OR NOT(B)"].into_iter().enumerate() {let raw=RawValue::from_integer((code as u128)<<14|0x40<<7|0x6f,32).unwrap();let f=r.fields.iter().find(|f|f.name=="Function").unwrap();assert_eq!(f.enum_name(&f.extract(&raw).unwrap()),Some(expected));assert_eq!(r.fields.iter().find(|f|f.name=="ResourceA").unwrap().extract(&raw).unwrap().integer().unwrap(),0x6f);assert_eq!(r.fields.iter().find(|f|f.name=="ResourceB").unwrap().extract(&raw).unwrap().integer().unwrap(),0x40);}}
    for (id,contains) in [("etm.claimset","支持掩码0xFF"),("etm.claimclr","当前共享claim状态")] {assert!(c.register(id).unwrap().fields.iter().find(|f|f.name=="TagBits").unwrap().description.contains(contains));}
}

#[test]
fn r52_gic_mmio_templates_enforce_capacity_ownership_and_mixed_field_permissions() {
    let c=load("cortex-r52-system-registers.toml");
    for n in [1u64,2,15,30] {
        let facts=[("gicd.present".to_owned(),1),("gicd.itlinesnumber".to_owned(),n)].into_iter().collect();
        let count=c.registers.iter().filter(|r|r.id.starts_with("gicd.") && r.implementation(&facts).0==Implementation::Yes).count();assert_eq!(count,15+49*n as usize);
    }
    for invalid in [0,31] {let facts=[("gicd.present".to_owned(),1),("gicd.itlinesnumber".to_owned(),invalid)].into_iter().collect();assert_eq!(c.register("gicd.irouter991").unwrap().implementation(&facts).0,Implementation::No);}
    for (id,offset,bits) in [("gicd.igroupr1",0x84,32),("gicd.igroupr30",0xf8,32),("gicd.ipriorityr247",0x7dc,32),("gicd.irouter32",0x6100,64),("gicd.irouter991",0x7ef8,64)] {let r=c.register(id).unwrap();assert_eq!(r.scope,Scope::Cluster);assert_eq!(r.bits,bits);assert!(matches!(&r.reader,Reader::Mmio{component,offset:o,require_owner_mapping:true} if component=="gicd" && *o==offset));assert!(r.writer.is_none());}
    assert!(c.register("gicd.igroupr0").is_none() && c.register("gicd.irouter992").is_none());
    let highest=c.register("gicd.igroupr30").unwrap();assert_eq!(highest.fields[0].name,"INTID960");assert_eq!(highest.fields[31].name,"INTID991");
    let priority=c.register("gicd.ipriorityr8").unwrap().fields.iter().find(|f|f.name=="Priority_INTID32").unwrap();assert_eq!((priority.segments[0].offset,priority.segments[0].width),(3,5));assert_eq!(priority.extract(&RawValue::from_integer(0xff,32).unwrap()).unwrap().integer().unwrap(),31);
    for (id,name,bit,access) in [("gicd.ctlr","RWP",31,Access::Ro),("gicr.ctlr","RWP",3,Access::Ro),("gicr.waker","ChildrenAsleep",2,Access::Ro),("gicr.waker","ProcessorSleep",1,Access::Rw)] {let f=c.register(id).unwrap().fields.iter().find(|f|f.name==name).unwrap();assert_eq!(f.segments[0].offset,bit);assert_eq!(f.access,Some(access));}
    let cfg=c.register("gicr.icfgr1").unwrap();for intid in 16..32 {let f=cfg.fields.iter().find(|f|f.name==format!("Trigger_INTID{intid}")).unwrap();assert_eq!(f.access,Some(if [16,17,18,19,20,21,28,29,31].contains(&intid) {Access::Rw} else {Access::Ro}));}
    assert_eq!(c.register("gicr.icfgr0").unwrap().access,Access::Ro);assert_eq!(c.register("gicr.icfgr0").unwrap().reset,Some(0xaaaaaaaa));
    for (id,offset) in [("gicr.typer",8),("gicr.igroupr0",0x10080),("gicr.ipriorityr7",0x1041c)] {let r=c.register(id).unwrap();assert_eq!(r.scope,Scope::Core);assert!(matches!(&r.reader,Reader::Mmio{component,offset:o,require_owner_mapping:true} if component=="gicr" && *o==offset));}
    let typer=c.register("gicr.typer").unwrap();assert_eq!(typer.bits,64);assert!(typer.reset.is_none());let aff0=typer.fields.iter().find(|f|f.name=="Aff0").unwrap();assert_eq!(aff0.extract(&RawValue::from_integer(3u128<<32,64).unwrap()).unwrap().integer().unwrap(),3);
    let export=c.register("gicr_export.typer").unwrap();assert_eq!(export.scope,Scope::Cluster);assert_eq!(export.access,Access::Ro);assert_eq!(export.fields.len(),14);assert_eq!(export.implementation(&Default::default()).0,Implementation::Unknown);assert!(export.description.contains("无PPI"));
    for (id,text) in [("gicd.isenabler1","W1S"),("gicd.icenabler1","W1C"),("gicr.icpendr0","W1C")] {let r=c.register(id).unwrap();assert!(r.fields.iter().any(|f|f.description.contains(text)));assert!(r.writer.is_none() && r.write.is_none());}
}

fn actual_list(catalogue:&Path,dir:&Path)->Value {
    fs::create_dir_all(dir).unwrap();
    let project=format!("version=2\n[gdb]\nexecutable={}\n[registers]\ncatalogue={}\n",serde_json::to_string(&dir.join("must-not-start-gdb.exe").to_string_lossy()).unwrap(),serde_json::to_string(&catalogue.to_string_lossy()).unwrap());
    let path=dir.join("debug.toml");fs::write(&path,project).unwrap();
    let input=json!({"id":1,"method":"registers_list","params":{}}).to_string()+"\n"+&json!({"id":2,"method":"quit","params":{}}).to_string()+"\n";
    let mut proc=Command::new(env!("CARGO_BIN_EXE_debugtui")).args(["--project"]).arg(&path).args(["--headless","--stdio"]).env("DEBUGTUI_CONFIG_DIR",dir.join("config")).stdin(Stdio::piped()).stdout(Stdio::piped()).stderr(Stdio::piped()).spawn().unwrap();
    proc.stdin.take().unwrap().write_all(input.as_bytes()).unwrap();let out=proc.wait_with_output().unwrap();
    fs::write(dir.join("stdout.jsonl"),&out.stdout).unwrap();fs::write(dir.join("stderr.txt"),&out.stderr).unwrap();
    assert!(out.status.success(),"{}",String::from_utf8_lossy(&out.stderr));
    let events:Vec<Value>=String::from_utf8(out.stdout).unwrap().lines().filter(|l|!l.trim().is_empty()).map(|l|serde_json::from_str(l).unwrap()).collect();
    assert!(!events.iter().any(|e|e["event"]=="log" && e["channel"]=="mi>"));
    let response=events.iter().find(|e|e["event"]=="response" && e["id"]==1).unwrap();assert_eq!(response["ok"],true,"{response}");
    assert!(response["result"]["probe"].is_null());response["result"].clone()
}

#[test]
fn actual_binary_lists_repaired_roots_with_no_debugger_and_retains_overrides() {
    let stamp=SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_nanos();
    let out=root().join("artifacts").join(format!("reference-fixes-{}-{stamp}",std::process::id()));
    for (core,name,expected) in [("r52","cortex-r52-system-registers.toml",2244),("f429","stm32f429-system-registers.toml",617)] {
        let path=root().join("reference").join(name);let before=fs::read(&path).unwrap();let result=actual_list(&path,&out.join(core));
        assert_eq!(result["catalogue"]["registers"].as_array().unwrap().len(),expected);
        assert_eq!(before,fs::read(&path).unwrap());
        if core=="f429" {
            assert!(result["definition_origins"]["nvic.ipr0"]["overrides"].as_str().unwrap().contains("armv7m-common"));
            let regs=result["catalogue"]["registers"].as_array().unwrap();let stim=regs.iter().find(|r|r["id"]=="itm.stim0").unwrap();assert_eq!(stim["write_fields"][0]["name"],"STIMULUS");assert_eq!(stim["fields"].as_array().unwrap().len(),2);
            let lar=regs.iter().find(|r|r["id"]=="etm.lar").unwrap();assert_eq!(lar["reader"]["kind"],"description_unsupported");assert!(lar.get("writer").is_none());
        }
    }
}
