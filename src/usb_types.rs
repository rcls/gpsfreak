use core::slice::from_raw_parts;

#[repr(packed)]
pub struct DeviceDesc {
    pub length            : u8,
    pub descriptor_type   : u8,
    pub usb               : u16,
    pub device_class      : u8,
    pub device_sub_class  : u8,
    pub device_protocol   : u8,
    pub max_packet_size0  : u8,
    pub vendor            : u16,
    pub product           : u16,
    pub device            : u16,
    pub i_manufacturer    : u8,
    pub i_product         : u8,
    pub i_serial          : u8,
    pub num_configurations: u8,
}
const _: () = const {assert!(size_of::<DeviceDesc>() == 18)};

#[repr(packed)]
pub struct ConfigurationDesc {
    pub length             : u8,
    pub descriptor_type    : u8,
    pub total_length       : u16,
    pub num_interfaces     : u8,
    pub configuration_value: u8,
    pub i_configuration    : u8,
    pub attributes         : u8,
    pub max_power          : u8,
}
const _: () = const {assert!(size_of::<ConfigurationDesc>() == 9)};

#[repr(packed)]
pub struct InterfaceAssociation {
    pub length            : u8,
    pub descriptor_type   : u8,
    pub first_interface   : u8,
    pub interface_count   : u8,
    pub function_class    : u8,
    pub function_sub_class: u8,
    pub function_protocol : u8,
    pub i_function        : u8,
}
const _: () = const {assert!(size_of::<InterfaceAssociation>() == 8)};

#[repr(packed)]
pub struct InterfaceDesc {
    pub length             : u8,
    pub descriptor_type    : u8,
    pub interface_number   : u8,
    pub alternate_setting  : u8,
    pub num_endpoints      : u8,
    pub interface_class    : u8,
    pub interface_sub_class: u8,
    pub interface_protocol : u8,
    pub i_interface        : u8,
    // .....
}
const _: () = const {assert!(size_of::<InterfaceDesc>() == 9)};

#[repr(packed)]
pub struct EndpointDesc {
    pub length             : u8,
    pub descriptor_type    : u8,
    pub endpoint_address   : u8,
    pub attributes         : u8,
    pub max_packet_size    : u16,
    pub interval           : u8,
}
const _: () = const {assert!(size_of::<EndpointDesc>() == 7)};

#[repr(packed)]
#[allow(non_camel_case_types)]
pub struct CDC_ACM_Continuation {
    pub cdc            : u16,
    pub call_management: u8,
    pub data_interface : u8,
    pub cdc_acm        : u8,
    pub cdc_union      : u8,
}

#[repr(packed)]
pub struct DeviceQualifier {
    pub length             : u8,
    pub descriptor_type    : u8,
    pub usb                : u16,
    pub device_class       : u8,
}

pub const TYPE_DEVICE       : u8 = 1;
pub const TYPE_CONFIGURATION: u8 = 2;
pub const TYPE_STRING       : u8 = 3;
pub const TYPE_INTERFACE    : u8 = 4;
pub const TYPE_ENDPOINT     : u8 = 5;
pub const TYPE_DEVICE_QUAL  : u8 = 6;
pub const TYPE_INTF_ASSOC   : u8 = 11;
pub const TYPE_CS_INTERFACE : u8 = 0x24;

#[repr(C)] // We keep the buffer aligned.
pub struct SetupHeader {
    pub request_type: u8,
    pub request     : u8,
    pub value_lo    : u8,
    pub value_hi    : u8,
    pub index       : u16,
    pub length      : u16,
}

pub type SetupResult = Result<&'static [u8], ()>;

pub fn setup_result<T>(data: &'static T) -> SetupResult {
    Ok(unsafe{from_raw_parts(data as *const _ as *const _, size_of::<T>())})
}

// CDC header
// CDC call management - capabilities = 3, bDataInterface = ?1.
// CDC ACM - capabilities = 0 to start.
// CDC union - slave not master

#[repr(packed)]
#[allow(non_camel_case_types)]
pub struct CDC_Header {
    pub length         : u8,
    pub descriptor_type: u8,
    pub sub_type       : u8,
    pub cdc            : u16,
}

#[repr(packed)]
pub struct UnionFunctionalDesc<const NUM_INTF: usize> {
    pub length           : u8,
    pub descriptor_type  : u8,
    pub sub_type         : u8,
    pub control_interface: u8,
    pub sub_interface    : [u8; NUM_INTF],
}

#[repr(packed)]
pub struct CallManagementDesc {
    pub length         : u8,
    pub descriptor_type: u8,
    pub sub_type       : u8,
    pub capabilities   : u8,
    pub data_interface : u8,
}

#[repr(packed)]
pub struct AbstractControlDesc {
    pub length         : u8,
    pub descriptor_type: u8,
    pub sub_type       : u8,
    pub capabilities   : u8,
}

#[repr(packed)]
pub struct LineCoding {
    pub dte_rate   : u32,
    pub char_format: u8,
    pub parity_type: u8,
    pub data_bits  : u8,
}
