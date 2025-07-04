MEMORY {
    BOOT2       : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH       : ORIGIN = 0x10000100, LENGTH = 2048K - 0x2100
    DEVICE_INFO : ORIGIN = 0x10ffe000, LENGTH = 0x1000
    WIFI_INFO   : ORIGIN = 0x10fff000, LENGTH = 0x1000
    RAM         : ORIGIN = 0x20000000, LENGTH = 256K
}

SECTIONS {
    .wifi_info : {
        KEEP(*(.wifi_info.*))
        *(.wifi_info.ssid);
        *(.wifi_info.pw);
        *(.wifi_info.ip);
        *(.wifi_info.port);
    } > WIFI_INFO
} INSERT AFTER .text;

SECTIONS {
    .device_info : {
        KEEP(*(.device_info.*))
        *(.device_info.id);
    } > DEVICE_INFO
} INSERT AFTER .text;