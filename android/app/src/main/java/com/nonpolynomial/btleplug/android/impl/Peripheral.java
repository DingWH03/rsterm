package com.nonpolynomial.btleplug.android.impl;

import android.annotation.SuppressLint;
import android.bluetooth.BluetoothAdapter;
import android.bluetooth.BluetoothDevice;
import android.bluetooth.BluetoothGatt;
import android.bluetooth.BluetoothGattCallback;
import android.bluetooth.BluetoothGattCharacteristic;
import android.bluetooth.BluetoothGattDescriptor;
import android.bluetooth.BluetoothGattService;
import java.lang.ref.WeakReference;
import java.util.ArrayList;
import java.util.Iterator;
import java.util.LinkedList;
import java.util.List;
import java.util.Queue;
import java.util.UUID;
import io.github.gedgygedgy.rust.future.Future;
import io.github.gedgygedgy.rust.future.SimpleFuture;
import io.github.gedgygedgy.rust.stream.QueueStream;
import io.github.gedgygedgy.rust.stream.Stream;

@SuppressWarnings({"unused", "deprecation"}) // Native code uses this class.
class Peripheral {
    private static final UUID CLIENT_CHARACTERISTIC_CONFIGURATION_DESCRIPTOR =
            new UUID(0x00002902_0000_1000L, 0x8000_00805f9b34fbL);

    private final BluetoothDevice device;
    private final Adapter adapter;
    private BluetoothGatt gatt;
    private final Callback callback;
    private boolean connected = false;

    // Cached connection parameters from onConnectionUpdated callback.
    private int connectionInterval = -1; // in 1.25 ms units
    private int connectionLatency = -1;
    private int supervisionTimeout = -1; // in 10 ms units

    private final Queue<Runnable> commandQueue = new LinkedList<>();
    private final LinkedList<WeakReference<QueueStream<BluetoothGattCharacteristic>>> notificationStreams =
            new LinkedList<>();
    private boolean executingCommand = false;
    private CommandCallback commandCallback;

    public Peripheral(Adapter adapter, String address) {
        BluetoothAdapter bluetoothAdapter = BluetoothAdapter.getDefaultAdapter();
        if (bluetoothAdapter == null) {
            throw new NoBluetoothAdapterException();
        }
        this.device = bluetoothAdapter.getRemoteDevice(address);
        this.adapter = adapter;
        this.callback = new Callback();
    }

    @SuppressLint("MissingPermission")
    public Future<Void> connect() {
        SimpleFuture<Void> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> this.asyncWithFuture(future, () -> {
                CommandCallback callback = new CommandCallback() {
                    @Override
                    public void onConnectionStateChange(BluetoothGatt gatt, int status, int newState) {
                        Peripheral.this.asyncWithFuture(future, () -> {
                            if (status != BluetoothGatt.GATT_SUCCESS) {
                                throw new NotConnectedException();
                            }
                            if (newState == BluetoothGatt.STATE_CONNECTED) {
                                Peripheral.this.wakeCommand(future, null);
                            }
                        });
                    }
                };
                if (this.connected) {
                    this.wakeCommand(future, null);
                } else if (this.gatt == null) {
                    try {
                        this.setCommandCallback(callback);
                        this.gatt = this.device.connectGatt(null, false, this.callback);
                    } catch (SecurityException ex) {
                        throw new PermissionDeniedException(ex);
                    }
                } else {
                    this.setCommandCallback(callback);
                    if (!this.gatt.connect()) {
                        throw new RuntimeException("Unable to reconnect to device");
                    }
                }
            }));
        }
        return future;
    }

    @SuppressLint("MissingPermission")
    public Future<Void> disconnect() {
        SimpleFuture<Void> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> this.asyncWithFuture(future, () -> {
                if (!this.connected || this.gatt == null) {
                    this.wakeCommand(future, null);
                } else {
                    this.setCommandCallback(new CommandCallback() {
                        @Override
                        public void onConnectionStateChange(BluetoothGatt gatt, int status, int newState) {
                            Peripheral.this.asyncWithFuture(future, () -> {
                                if (status != BluetoothGatt.GATT_SUCCESS) {
                                    throw new RuntimeException("Unable to disconnect");
                                }
                                if (newState == BluetoothGatt.STATE_DISCONNECTED) {
                                    Peripheral.this.gatt.close();
                                    Peripheral.this.gatt = null;
                                    Peripheral.this.wakeCommand(future, null);
                                }
                            });
                        }
                    });
                    this.gatt.disconnect();
                }
            }));
        }
        return future;
    }

    public boolean isConnected() {
        return this.connected;
    }

    @SuppressLint("MissingPermission")
    public String getDeviceName() {
        try {
            return this.device.getName();
        } catch (SecurityException ex) {
            return null;
        }
    }

    public synchronized int[] getConnectionParameters() {
        if (this.connectionInterval < 0) {
            return null;
        }
        return new int[] {this.connectionInterval, this.connectionLatency, this.supervisionTimeout};
    }

    @SuppressLint("MissingPermission")
    public synchronized boolean requestConnectionPriority(int priority) {
        if (!this.connected || this.gatt == null) {
            throw new NotConnectedException();
        }
        return this.gatt.requestConnectionPriority(priority);
    }

    @SuppressLint("MissingPermission")
    public Future<Integer> requestMtu(int mtu) {
        SimpleFuture<Integer> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> this.asyncWithFuture(future, () -> {
                if (!this.connected || this.gatt == null) {
                    throw new NotConnectedException();
                }
                this.setCommandCallback(new CommandCallback() {
                    @Override
                    public void onMtuChanged(BluetoothGatt gatt, int mtu, int status) {
                        Peripheral.this.asyncWithFuture(future, () -> {
                            if (status != BluetoothGatt.GATT_SUCCESS) {
                                throw new RuntimeException("MTU negotiation failed");
                            }
                            Peripheral.this.wakeCommand(future, mtu);
                        });
                    }
                });
                if (!this.gatt.requestMtu(mtu)) {
                    throw new RuntimeException("Unable to request MTU");
                }
            }));
        }
        return future;
    }

    @SuppressLint("MissingPermission")
    public Future<byte[]> read(UUID uuid) {
        SimpleFuture<byte[]> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> this.asyncWithFuture(future, () -> {
                if (!this.connected || this.gatt == null) {
                    throw new NotConnectedException();
                }
                BluetoothGattCharacteristic characteristic = this.getCharacteristicByUuid(uuid);
                this.setCommandCallback(new CommandCallback() {
                    @Override
                    public void onCharacteristicRead(BluetoothGatt gatt, BluetoothGattCharacteristic characteristic, int status) {
                        Peripheral.this.asyncWithFuture(future, () -> {
                            if (status != BluetoothGatt.GATT_SUCCESS) {
                                throw new RuntimeException("Unable to read characteristic");
                            }
                            if (!characteristic.getUuid().equals(uuid)) {
                                throw new UnexpectedCharacteristicException();
                            }
                            Peripheral.this.wakeCommand(future, characteristic.getValue());
                        });
                    }
                });
                if (!this.gatt.readCharacteristic(characteristic)) {
                    throw new RuntimeException("Unable to read characteristic");
                }
            }));
        }
        return future;
    }

    @SuppressLint("MissingPermission")
    public Future<Void> write(UUID uuid, byte[] data, int writeType) {
        SimpleFuture<Void> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> this.asyncWithFuture(future, () -> {
                if (!this.connected || this.gatt == null) {
                    throw new NotConnectedException();
                }
                BluetoothGattCharacteristic characteristic = this.getCharacteristicByUuid(uuid);
                characteristic.setValue(data);
                characteristic.setWriteType(writeType);
                this.setCommandCallback(new CommandCallback() {
                    @Override
                    public void onCharacteristicWrite(BluetoothGatt gatt, BluetoothGattCharacteristic characteristic, int status) {
                        Peripheral.this.asyncWithFuture(future, () -> {
                            if (status != BluetoothGatt.GATT_SUCCESS) {
                                throw new RuntimeException("Unable to write characteristic");
                            }
                            if (!characteristic.getUuid().equals(uuid)) {
                                throw new UnexpectedCharacteristicException();
                            }
                            Peripheral.this.wakeCommand(future, null);
                        });
                    }
                });
                if (!this.gatt.writeCharacteristic(characteristic)) {
                    throw new RuntimeException("Unable to write characteristic");
                }
            }));
        }
        return future;
    }

    @SuppressLint("MissingPermission")
    public Future<List<BluetoothGattService>> discoverServices() {
        SimpleFuture<List<BluetoothGattService>> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> this.asyncWithFuture(future, () -> {
                if (!this.connected || this.gatt == null) {
                    throw new NotConnectedException();
                }
                this.setCommandCallback(new CommandCallback() {
                    @Override
                    public void onServicesDiscovered(BluetoothGatt gatt, int status) {
                        Peripheral.this.asyncWithFuture(future, () -> {
                            if (status != BluetoothGatt.GATT_SUCCESS) {
                                throw new RuntimeException("Unable to discover services");
                            }
                            Peripheral.this.wakeCommand(future, gatt.getServices());
                        });
                    }
                });
                if (!this.gatt.discoverServices()) {
                    throw new RuntimeException("Unable to discover services");
                }
            }));
        }
        return future;
    }

    @SuppressLint("MissingPermission")
    public Future<Void> setCharacteristicNotification(UUID uuid, boolean enable) {
        SimpleFuture<Void> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> this.asyncWithFuture(future, () -> {
                if (!this.connected || this.gatt == null) {
                    throw new NotConnectedException();
                }
                BluetoothGattCharacteristic characteristic = this.getCharacteristicByUuid(uuid);
                if (!this.gatt.setCharacteristicNotification(characteristic, enable)) {
                    throw new RuntimeException("Unable to set characteristic notification");
                }
                BluetoothGattDescriptor descriptor =
                        characteristic.getDescriptor(CLIENT_CHARACTERISTIC_CONFIGURATION_DESCRIPTOR);
                if (descriptor == null) {
                    throw new NoSuchCharacteristicException();
                }
                byte[] cccdValue;
                if (!enable) {
                    cccdValue = BluetoothGattDescriptor.DISABLE_NOTIFICATION_VALUE;
                } else if ((characteristic.getProperties() & BluetoothGattCharacteristic.PROPERTY_INDICATE) != 0) {
                    cccdValue = BluetoothGattDescriptor.ENABLE_INDICATION_VALUE;
                } else {
                    cccdValue = BluetoothGattDescriptor.ENABLE_NOTIFICATION_VALUE;
                }
                descriptor.setValue(cccdValue);
                this.setCommandCallback(new CommandCallback() {
                    @Override
                    public void onDescriptorWrite(BluetoothGatt gatt, BluetoothGattDescriptor descriptor, int status) {
                        Peripheral.this.asyncWithFuture(future, () -> {
                            if (status != BluetoothGatt.GATT_SUCCESS) {
                                throw new RuntimeException("Unable to write descriptor");
                            }
                            if (!descriptor.getUuid().equals(CLIENT_CHARACTERISTIC_CONFIGURATION_DESCRIPTOR)
                                    || !descriptor.getCharacteristic().getUuid().equals(uuid)) {
                                throw new UnexpectedCharacteristicException();
                            }
                            Peripheral.this.wakeCommand(future, null);
                        });
                    }
                });
                if (!this.gatt.writeDescriptor(descriptor)) {
                    throw new RuntimeException("Unable to write client characteristic configuration descriptor");
                }
            }));
        }
        return future;
    }

    public Stream<BluetoothGattCharacteristic> getNotifications() {
        QueueStream<BluetoothGattCharacteristic> stream = new QueueStream<>();
        synchronized (this) {
            this.notificationStreams.add(new WeakReference<>(stream));
        }
        return stream;
    }

    @SuppressLint("MissingPermission")
    public Future<byte[]> readDescriptor(UUID characteristicUuid, UUID uuid) {
        SimpleFuture<byte[]> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> this.asyncWithFuture(future, () -> {
                if (!this.connected || this.gatt == null) {
                    throw new NotConnectedException();
                }
                BluetoothGattDescriptor descriptor = this.getDescriptorByUuid(characteristicUuid, uuid);
                this.setCommandCallback(new CommandCallback() {
                    @Override
                    public void onDescriptorRead(BluetoothGatt gatt, BluetoothGattDescriptor descriptor, int status) {
                        Peripheral.this.asyncWithFuture(future, () -> {
                            if (status != BluetoothGatt.GATT_SUCCESS) {
                                throw new RuntimeException("Unable to read descriptor");
                            }
                            if (!descriptor.getUuid().equals(uuid)) {
                                throw new UnexpectedCharacteristicException();
                            }
                            Peripheral.this.wakeCommand(future, descriptor.getValue());
                        });
                    }
                });
                if (!this.gatt.readDescriptor(descriptor)) {
                    throw new RuntimeException("Unable to read descriptor");
                }
            }));
        }
        return future;
    }

    @SuppressLint("MissingPermission")
    public Future<Void> writeDescriptor(UUID characteristicUuid, UUID uuid, byte[] data) {
        SimpleFuture<Void> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> this.asyncWithFuture(future, () -> {
                if (!this.connected || this.gatt == null) {
                    throw new NotConnectedException();
                }
                BluetoothGattDescriptor descriptor = this.getDescriptorByUuid(characteristicUuid, uuid);
                descriptor.setValue(data);
                this.setCommandCallback(new CommandCallback() {
                    @Override
                    public void onDescriptorWrite(BluetoothGatt gatt, BluetoothGattDescriptor descriptor, int status) {
                        Peripheral.this.asyncWithFuture(future, () -> {
                            if (status != BluetoothGatt.GATT_SUCCESS) {
                                throw new RuntimeException("Unable to write descriptor");
                            }
                            if (!descriptor.getUuid().equals(uuid)) {
                                throw new UnexpectedCharacteristicException();
                            }
                            Peripheral.this.wakeCommand(future, null);
                        });
                    }
                });
                if (!this.gatt.writeDescriptor(descriptor)) {
                    throw new RuntimeException("Unable to write descriptor");
                }
            }));
        }
        return future;
    }

    @SuppressLint("MissingPermission")
    public Future<Integer> readRemoteRssi() {
        SimpleFuture<Integer> future = new SimpleFuture<>();
        synchronized (this) {
            this.queueCommand(() -> this.asyncWithFuture(future, () -> {
                if (!this.connected || this.gatt == null) {
                    throw new NotConnectedException();
                }
                this.setCommandCallback(new CommandCallback() {
                    @Override
                    public void onReadRemoteRssi(BluetoothGatt gatt, int rssi, int status) {
                        Peripheral.this.asyncWithFuture(future, () -> {
                            if (status != BluetoothGatt.GATT_SUCCESS) {
                                throw new RuntimeException("RSSI read failed");
                            }
                            Peripheral.this.wakeCommand(future, rssi);
                        });
                    }
                });
                if (!this.gatt.readRemoteRssi()) {
                    throw new RuntimeException("Unable to read remote RSSI");
                }
            }));
        }
        return future;
    }

    private List<BluetoothGattCharacteristic> getCharacteristics() {
        List<BluetoothGattCharacteristic> result = new ArrayList<>();
        if (this.gatt != null) {
            for (BluetoothGattService service : this.gatt.getServices()) {
                result.addAll(service.getCharacteristics());
            }
        }
        return result;
    }

    private BluetoothGattCharacteristic getCharacteristicByUuid(UUID uuid) {
        for (BluetoothGattCharacteristic characteristic : this.getCharacteristics()) {
            if (characteristic.getUuid().equals(uuid)) {
                return characteristic;
            }
        }
        throw new NoSuchCharacteristicException();
    }

    private BluetoothGattDescriptor getDescriptorByUuid(UUID characteristicUuid, UUID uuid) {
        BluetoothGattCharacteristic characteristic = this.getCharacteristicByUuid(characteristicUuid);
        for (BluetoothGattDescriptor descriptor : characteristic.getDescriptors()) {
            if (descriptor.getUuid().equals(uuid)) {
                return descriptor;
            }
        }
        throw new NoSuchCharacteristicException();
    }

    private void queueCommand(Runnable callback) {
        if (this.executingCommand) {
            this.commandQueue.add(callback);
        } else {
            this.executingCommand = true;
            callback.run();
        }
    }

    private void setCommandCallback(CommandCallback callback) {
        assert this.commandCallback == null;
        this.commandCallback = callback;
    }

    private void runNextCommand() {
        assert this.executingCommand;
        this.commandCallback = null;
        if (this.commandQueue.isEmpty()) {
            this.executingCommand = false;
        } else {
            Runnable callback = this.commandQueue.remove();
            callback.run();
        }
    }

    private <T> void wakeCommand(SimpleFuture<T> future, T result) {
        future.wake(result);
        this.runNextCommand();
    }

    private <T> void asyncWithFuture(SimpleFuture<T> future, Runnable callback) {
        try {
            callback.run();
        } catch (Throwable ex) {
            future.wakeWithThrowable(ex);
            this.runNextCommand();
        }
    }

    private class Callback extends BluetoothGattCallback {
        @Override
        public void onConnectionStateChange(BluetoothGatt gatt, int status, int newState) {
            synchronized (Peripheral.this) {
                switch (newState) {
                    case BluetoothGatt.STATE_CONNECTED:
                        Peripheral.this.connected = true;
                        break;
                    case BluetoothGatt.STATE_DISCONNECTED:
                        Peripheral.this.connected = false;
                        break;
                    default:
                        break;
                }
                if (Peripheral.this.commandCallback != null) {
                    Peripheral.this.commandCallback.onConnectionStateChange(gatt, status, newState);
                }
            }
            switch (newState) {
                case BluetoothGatt.STATE_CONNECTED:
                    Peripheral.this.adapter.onConnectionStateChanged(Peripheral.this.device.getAddress(), true);
                    break;
                case BluetoothGatt.STATE_DISCONNECTED:
                    Peripheral.this.adapter.onConnectionStateChanged(Peripheral.this.device.getAddress(), false);
                    break;
                default:
                    break;
            }
        }

        @Override
        public void onCharacteristicRead(BluetoothGatt gatt, BluetoothGattCharacteristic characteristic, int status) {
            synchronized (Peripheral.this) {
                if (Peripheral.this.commandCallback != null) {
                    Peripheral.this.commandCallback.onCharacteristicRead(gatt, characteristic, status);
                }
            }
        }

        @Override
        public void onCharacteristicRead(
                BluetoothGatt gatt,
                BluetoothGattCharacteristic characteristic,
                byte[] value,
                int status) {
            characteristic.setValue(value);
            this.onCharacteristicRead(gatt, characteristic, status);
        }

        @Override
        public void onCharacteristicWrite(BluetoothGatt gatt, BluetoothGattCharacteristic characteristic, int status) {
            synchronized (Peripheral.this) {
                if (Peripheral.this.commandCallback != null) {
                    Peripheral.this.commandCallback.onCharacteristicWrite(gatt, characteristic, status);
                }
            }
        }

        @Override
        public void onServicesDiscovered(BluetoothGatt gatt, int status) {
            synchronized (Peripheral.this) {
                if (Peripheral.this.commandCallback != null) {
                    Peripheral.this.commandCallback.onServicesDiscovered(gatt, status);
                }
            }
        }

        @Override
        public void onCharacteristicChanged(BluetoothGatt gatt, BluetoothGattCharacteristic characteristic) {
            Peripheral.this.reportNotification(characteristic.getUuid(), characteristic.getProperties(), characteristic.getPermissions(), characteristic.getValue());
        }

        @Override
        public void onCharacteristicChanged(
                BluetoothGatt gatt,
                BluetoothGattCharacteristic characteristic,
                byte[] value) {
            Peripheral.this.reportNotification(characteristic.getUuid(), characteristic.getProperties(), characteristic.getPermissions(), value);
        }

        @Override
        public void onDescriptorRead(BluetoothGatt gatt, BluetoothGattDescriptor descriptor, int status) {
            synchronized (Peripheral.this) {
                if (Peripheral.this.commandCallback != null) {
                    Peripheral.this.commandCallback.onDescriptorRead(gatt, descriptor, status);
                }
            }
        }

        @Override
        public void onDescriptorRead(
                BluetoothGatt gatt,
                BluetoothGattDescriptor descriptor,
                int status,
                byte[] value) {
            descriptor.setValue(value);
            this.onDescriptorRead(gatt, descriptor, status);
        }

        @Override
        public void onDescriptorWrite(BluetoothGatt gatt, BluetoothGattDescriptor descriptor, int status) {
            synchronized (Peripheral.this) {
                if (Peripheral.this.commandCallback != null) {
                    Peripheral.this.commandCallback.onDescriptorWrite(gatt, descriptor, status);
                }
            }
        }

        @Override
        public void onMtuChanged(BluetoothGatt gatt, int mtu, int status) {
            synchronized (Peripheral.this) {
                if (Peripheral.this.commandCallback != null) {
                    Peripheral.this.commandCallback.onMtuChanged(gatt, mtu, status);
                }
            }
        }

        @Override
        public void onReadRemoteRssi(BluetoothGatt gatt, int rssi, int status) {
            synchronized (Peripheral.this) {
                if (Peripheral.this.commandCallback != null) {
                    Peripheral.this.commandCallback.onReadRemoteRssi(gatt, rssi, status);
                }
            }
        }

        // Hidden API in BluetoothGattCallback; do not mark with @Override.
        public void onConnectionUpdated(BluetoothGatt gatt, int interval, int latency, int timeout, int status) {
            if (status == BluetoothGatt.GATT_SUCCESS) {
                synchronized (Peripheral.this) {
                    Peripheral.this.connectionInterval = interval;
                    Peripheral.this.connectionLatency = latency;
                    Peripheral.this.supervisionTimeout = timeout;
                }
            }
        }
    }

    private void reportNotification(UUID uuid, int properties, int permissions, byte[] value) {
        BluetoothGattCharacteristic copy = new BluetoothGattCharacteristic(uuid, properties, permissions);
        copy.setValue(value);
        synchronized (this) {
            Iterator<WeakReference<QueueStream<BluetoothGattCharacteristic>>> iter = this.notificationStreams.iterator();
            while (iter.hasNext()) {
                QueueStream<BluetoothGattCharacteristic> stream = iter.next().get();
                if (stream != null) {
                    stream.add(copy);
                } else {
                    iter.remove();
                }
            }
        }
    }

    private static abstract class CommandCallback extends BluetoothGattCallback {
        @Override
        public void onConnectionStateChange(BluetoothGatt gatt, int status, int newState) {
            throw new UnexpectedCallbackException();
        }

        @Override
        public void onCharacteristicRead(BluetoothGatt gatt, BluetoothGattCharacteristic characteristic, int status) {
            throw new UnexpectedCallbackException();
        }

        @Override
        public void onCharacteristicWrite(BluetoothGatt gatt, BluetoothGattCharacteristic characteristic, int status) {
            throw new UnexpectedCallbackException();
        }

        @Override
        public void onDescriptorRead(BluetoothGatt gatt, BluetoothGattDescriptor descriptor, int status) {
            throw new UnexpectedCallbackException();
        }

        @Override
        public void onServicesDiscovered(BluetoothGatt gatt, int status) {
            throw new UnexpectedCallbackException();
        }

        @Override
        public void onDescriptorWrite(BluetoothGatt gatt, BluetoothGattDescriptor descriptor, int status) {
            throw new UnexpectedCallbackException();
        }

        @Override
        public void onMtuChanged(BluetoothGatt gatt, int mtu, int status) {
            throw new UnexpectedCallbackException();
        }

        @Override
        public void onReadRemoteRssi(BluetoothGatt gatt, int rssi, int status) {
            throw new UnexpectedCallbackException();
        }
    }
}
