use super::types::*;
use super::strings::string_index;

pub const INTF_ACM_INTR: u8 = 0;
pub const INTF_ACM_DATA: u8 = 1;
pub const INTF_MAIN    : u8 = 2;
pub const INTF_DFU     : u8 = 3;

pub static DEVICE_DESC: DeviceDesc = DeviceDesc{
    length            : size_of::<DeviceDesc>() as u8,
    descriptor_type   : TYPE_DEVICE,
    usb               : 0x200,
    device_class      : 239, // Miscellaneous device
    device_sub_class  : 2, // Unknown
    device_protocol   : 1, // Interface association
    max_packet_size0  : 64,
    vendor            : 0x1209,
    product           : 0xce93,
    device            : 0x100,
    i_manufacturer    : string_index("Ralph"),
    i_product         : string_index("GPS Freak"),
    i_serial          : super::strings::IDX_SERIAL_NUMBER,
    num_configurations: 1,
};

#[repr(packed)]
#[allow(dead_code)]
pub struct FullConfigDesc {
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
    endp3     : EndpointDesc,
    endp4     : EndpointDesc,
    interface3: InterfaceDesc,
    dfu       : DFU_FunctionalDesc,
}

/// Our main configuration descriptor.
pub static CONFIG0_DESC: FullConfigDesc = FullConfigDesc{
    config: ConfigurationDesc{
        length             : size_of::<ConfigurationDesc>() as u8,
        descriptor_type    : TYPE_CONFIGURATION,
        total_length       : size_of::<FullConfigDesc>() as u16,
        num_interfaces     : 4,
        configuration_value: 1,
        i_configuration    : string_index("Device Configuration"),
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
    // 1 endpoints, Communication, Abstract, AT Commands [sic]
    interface0: InterfaceDesc::new(
        INTF_ACM_INTR, 1, 2, 2, 1, string_index("CDC")),
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
        // TODO - this is not correct
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
    interface1: InterfaceDesc::new(
        INTF_ACM_DATA, 2, 10, 0, 0, string_index("CDC DATA interface")),
    endp1: EndpointDesc::new(0x81, 2, 64, 1), // IN 1, Bulk.
    endp2: EndpointDesc::new(0x01, 2, 64, 1), // OUT 1, Bulk.
    interface2: InterfaceDesc::new(                          // Vendor specific.
        INTF_MAIN, 2, 0xff, 0, 0, string_index("Device Control")),
    endp3: EndpointDesc::new(0x03, 2, 64, 1),
    endp4: EndpointDesc::new(0x83, 2, 64, 1),
    interface3: InterfaceDesc::new(           // Application specific / DFU / 1.
        INTF_DFU, 0, 0xfe, 1, 1, string_index("DFU")),
    dfu: DFU_FunctionalDesc {
        length             : size_of::<DFU_FunctionalDesc>() as u8,
        descriptor_type    : TYPE_DFU_FUNCTIONAL,
        attributes         : 0x0b,
        detach_time_out    : 1000,
        transfer_size      : 1024,
        dfu_version        : 0x011a,
    },
};
