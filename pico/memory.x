MEMORY {
    BOOT2 : ORIGIN = 0x10000000, LENGTH = 0x100
    FLASH : ORIGIN = 0x10000100, LENGTH = 2048K - 0x1100
    WIFI_INFO : ORIGIN = 0x10ff0000, LENGTH = 0x1000
    RAM   : ORIGIN = 0x20000000, LENGTH = 256K
}

SECTIONS {
     .wifi_info :  {
       *(.wifi_info);
       . = ALIGN(4);
     } > WIFI_INFO
} INSERT AFTER .text;