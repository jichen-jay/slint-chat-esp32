#include <esp_wifi.h>

// SPI bindings for ST7789 display
#include <driver/spi_master.h>
#include <driver/gpio.h>

// I2S bindings for audio recording
#include <driver/i2s.h>
#include <driver/i2s_std.h>

// SD card and file system bindings
#include <esp_vfs_fat.h>
#include <sdmmc_cmd.h>
#include <driver/sdmmc_host.h>

// Additional system headers for audio processing
#include <esp_heap_caps.h>
#include <freertos/FreeRTOS.h>
#include <freertos/task.h>