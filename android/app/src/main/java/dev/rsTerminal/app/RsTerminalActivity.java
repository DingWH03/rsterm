package dev.rsTerminal.app;

import android.app.NativeActivity;
import android.os.Build;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;

public class RsTerminalActivity extends NativeActivity {
    private Object backCallback = null;

    @Override
    protected void onStart() {
        super.onStart();
        registerBackCallback();
    }

    @Override
    protected void onStop() {
        unregisterBackCallback();
        super.onStop();
    }

    @Override
    public void onBackPressed() {
        // Fallback for older API levels (pre-33).
        nativeOnBackPressed();
    }

    private void registerBackCallback() {
        if (Build.VERSION.SDK_INT < 33 || backCallback != null) {
            return;
        }
        OnBackInvokedCallback callback = () -> nativeOnBackPressed();
        backCallback = callback;
        getOnBackInvokedDispatcher().registerOnBackInvokedCallback(
            OnBackInvokedDispatcher.PRIORITY_DEFAULT,
            callback
        );
    }

    private void unregisterBackCallback() {
        if (Build.VERSION.SDK_INT < 33 || backCallback == null) {
            return;
        }
        getOnBackInvokedDispatcher().unregisterOnBackInvokedCallback((OnBackInvokedCallback) backCallback);
        backCallback = null;
    }

    private static native void nativeOnBackPressed();
}
