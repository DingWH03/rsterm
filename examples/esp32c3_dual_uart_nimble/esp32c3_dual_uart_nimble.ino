#include <Arduino.h>
#include <NimBLEDevice.h>

#define DEVICE_NAME "ESP32C3-DUAL-UART"

#define UART0_RX_PIN 20
#define UART0_TX_PIN 21
#define UART1_RX_PIN 18
#define UART1_TX_PIN 19

#define UART0_BAUD 115200
#define UART1_BAUD 115200
#define BLE_NOTIFY_CHUNK 20

static const char* BLE_UART_SERVICE_UUID = "B7E40001-B5A3-F393-E0A9-E50E24DCCA9E";
static const char* UART0_TX_UUID = "B7E40002-B5A3-F393-E0A9-E50E24DCCA9E";
static const char* UART0_RX_UUID = "B7E40003-B5A3-F393-E0A9-E50E24DCCA9E";
static const char* UART1_TX_UUID = "B7E40004-B5A3-F393-E0A9-E50E24DCCA9E";
static const char* UART1_RX_UUID = "B7E40005-B5A3-F393-E0A9-E50E24DCCA9E";

static NimBLECharacteristic* uart0TxChar = nullptr;
static NimBLECharacteristic* uart1TxChar = nullptr;
static volatile bool bleConnected = false;

class ServerCallbacks : public NimBLEServerCallbacks {
public:
    void onConnect(NimBLEServer*, NimBLEConnInfo&) override {
        bleConnected = true;
    }

    void onDisconnect(NimBLEServer*, NimBLEConnInfo&, int) override {
        bleConnected = false;
        NimBLEDevice::startAdvertising();
    }
};

class UartRxCallbacks : public NimBLECharacteristicCallbacks {
public:
    explicit UartRxCallbacks(HardwareSerial& serial) : serial_(&serial) {}

    void onWrite(NimBLECharacteristic* characteristic, NimBLEConnInfo&) override {
        NimBLEAttValue value = characteristic->getValue();
        if (value.size() > 0) {
            serial_->write(value.data(), value.size());
        }
    }

private:
    HardwareSerial* serial_;
};

static ServerCallbacks serverCallbacks;
static UartRxCallbacks uart0RxCallbacks(Serial0);
static UartRxCallbacks uart1RxCallbacks(Serial1);

static void pumpUartToBle(HardwareSerial& serial, NimBLECharacteristic* txChar) {
    if (!bleConnected || txChar == nullptr) {
        return;
    }

    uint8_t buf[BLE_NOTIFY_CHUNK];
    while (serial.available() > 0) {
        size_t want = serial.available();
        if (want > sizeof(buf)) {
            want = sizeof(buf);
        }
        size_t n = serial.read(buf, want);
        if (n == 0) {
            break;
        }
        if (!txChar->notify(buf, n)) {
            break;
        }
        delay(1);
    }
}

void setup() {
    Serial0.begin(UART0_BAUD, SERIAL_8N1, UART0_RX_PIN, UART0_TX_PIN);
    Serial1.begin(UART1_BAUD, SERIAL_8N1, UART1_RX_PIN, UART1_TX_PIN);
    Serial0.setRxTimeout(1);
    Serial1.setRxTimeout(1);

    NimBLEDevice::init(DEVICE_NAME);
    NimBLEDevice::setMTU(247);

    NimBLEServer* server = NimBLEDevice::createServer();
    server->setCallbacks(&serverCallbacks);

    NimBLEService* service = server->createService(BLE_UART_SERVICE_UUID);

    uart0TxChar = service->createCharacteristic(UART0_TX_UUID, NIMBLE_PROPERTY::READ | NIMBLE_PROPERTY::NOTIFY);
    NimBLECharacteristic* uart0RxChar = service->createCharacteristic(UART0_RX_UUID, NIMBLE_PROPERTY::WRITE | NIMBLE_PROPERTY::WRITE_NR);
    uart0RxChar->setCallbacks(&uart0RxCallbacks);

    uart1TxChar = service->createCharacteristic(UART1_TX_UUID, NIMBLE_PROPERTY::READ | NIMBLE_PROPERTY::NOTIFY);
    NimBLECharacteristic* uart1RxChar = service->createCharacteristic(UART1_RX_UUID, NIMBLE_PROPERTY::WRITE | NIMBLE_PROPERTY::WRITE_NR);
    uart1RxChar->setCallbacks(&uart1RxCallbacks);

    service->start();

    NimBLEAdvertising* advertising = NimBLEDevice::getAdvertising();
    advertising->setName(DEVICE_NAME);
    advertising->addServiceUUID(BLE_UART_SERVICE_UUID);
    advertising->enableScanResponse(true);
    NimBLEDevice::startAdvertising();
}

void loop() {
    pumpUartToBle(Serial0, uart0TxChar);
    pumpUartToBle(Serial1, uart1TxChar);
    delay(1);
}
