use super::types::*;
use super::strings::string_index;

pub static DEVICE_DESC: DeviceDesc = DeviceDesc{
    length            : size_of::<DeviceDesc>() as u8,
    descriptor_type   : TYPE_DEVICE,
    usb               : 0x200,
    device_class      : 239, // Miscellaneous device
    device_sub_class  : 2, // Unknown
    device_protocol   : 1, // Interface association
    max_packet_size0  : 64,
    vendor            : 0xf055, // FIXME
    product           : 0xd448, // FIXME
    device            : 0x100,
    i_manufacturer    : string_index("Ralph"),
    i_product         : string_index("GPS REF"),
    i_serial          : string_index("0000"),
    num_configurations: 1,
};

#[repr(packed)]
#[allow(dead_code)]
pub struct Config1ACMCDCplus2 {
    config    : ConfigurationDesc,
    assoc     : InterfaceAssociation,
    interface0: InterfaceDesc,
    cdc_header: CDC_Header,
    call_mgmt : CallManagementDesc,
    acm_ctrl  : AbstractControlDesc,
    union_desc: UnionFunctionalDesc<1>,
    endp0     : EndpointDesc,
    interface1: InterfaceDesc,
    endp1     : EndpointDesc,
    endp2     : EndpointDesc,
    interface2: InterfaceDesc,
    dfu       : DFU_FunctionalDesc,
}

/// Our main configuration descriptor.
pub static CONFIG0_DESC: Config1ACMCDCplus2 = Config1ACMCDCplus2{
    config: ConfigurationDesc{
        length             : size_of::<ConfigurationDesc>() as u8,
        descriptor_type    : TYPE_CONFIGURATION,
        total_length       : size_of::<Config1ACMCDCplus2>() as u16,
        num_interfaces     : 3,
        configuration_value: 1,
        i_configuration    : string_index("Single ACM"),
        attributes         : 0x80,      // Bus powered.
        max_power          : 200,       // 400mA
    },
    assoc: InterfaceAssociation{
        length            : size_of::<InterfaceAssociation>() as u8,
        descriptor_type   : TYPE_INTF_ASSOC,
        first_interface   : 0,
        interface_count   : 2,
        function_class    : 2,          // Communications
        function_sub_class: 2,          // Abstract (Modem [sic])
        function_protocol : 0,
        i_function        : string_index("CDC"),
    },
    interface0: InterfaceDesc{
        length             : size_of::<InterfaceDesc>() as u8,
        descriptor_type    : TYPE_INTERFACE,
        interface_number   : 0,
        alternate_setting  : 0,
        num_endpoints      : 1,
        interface_class    : 2,         // Communications
        interface_sub_class: 2,         // Abstract
        interface_protocol : 1,         // AT Commands [sic]
        i_interface        : string_index("CDC"),
    },
    cdc_header: CDC_Header{
        length             : size_of::<CDC_Header>() as u8,
        descriptor_type    : TYPE_CS_INTERFACE,
        sub_type           : 0,         // CDC Header Functional Descriptor
        cdc                : 0x0110,
    },
    call_mgmt: CallManagementDesc{
        length             : size_of::<CallManagementDesc>() as u8,
        descriptor_type    : TYPE_CS_INTERFACE,
        sub_type           : 1,         // Call management [sic]
        capabilities       : 3,         // Call management, data.
        data_interface     : 1,
    },
    acm_ctrl: AbstractControlDesc{
        length             : size_of::<AbstractControlDesc>() as u8,
        descriptor_type    : TYPE_CS_INTERFACE,
        sub_type           : 2,         // Abstract Control Mgmt Functional Desc
        capabilities       : 6,         // "Line coding and serial state"
    },
    union_desc: UnionFunctionalDesc::<1>{
        length             : size_of::<UnionFunctionalDesc<1>>() as u8,
        descriptor_type    : TYPE_CS_INTERFACE,
        sub_type           : 6,         // Union Functional Desc,
        control_interface  : 0,
        sub_interface      : [1],
    },
    endp0: EndpointDesc::new(0x82, 3, 64, 4), // IN 2, Interrupt.
    interface1: InterfaceDesc{
        length             : size_of::<InterfaceDesc>() as u8,
        descriptor_type    : TYPE_INTERFACE,
        interface_number   : 1,
        alternate_setting  : 0,
        num_endpoints      : 2,
        interface_class    : 10,        // CDC data
        interface_sub_class: 0,
        interface_protocol : 0,
        i_interface        : string_index("CDC DATA interface"),
    },
    endp1: EndpointDesc::new(0x81, 2, 64, 1), // IN 1, Bulk.
    endp2: EndpointDesc::new(0x01, 2, 64, 1), // OUT 82, Bulk.
    interface2: InterfaceDesc{
        length             : size_of::<InterfaceDesc>() as u8,
        descriptor_type    : TYPE_INTERFACE,
        interface_number   : 2,
        alternate_setting  : 0,
        num_endpoints      : 0,
        interface_class    : 0xfe,      // Application specific
        interface_sub_class: 1,         // Device Firmware Upgrade
        interface_protocol : 1,         // Runtime
        i_interface        : string_index("DFU"),
    },
    dfu: DFU_FunctionalDesc {
        length             : size_of::<DFU_FunctionalDesc>() as u8,
        descriptor_type    : TYPE_DFU_FUNCTIONAL,
        attributes         : 0x0b,
        detach_time_out    : 1000,
        transfer_size      : 1024,
        dfu_version        : 0x011a,
    },
};
