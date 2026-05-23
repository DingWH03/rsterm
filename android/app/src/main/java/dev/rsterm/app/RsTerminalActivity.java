package dev.rsTerminal.app;

import android.app.NativeActivity;
import android.window.OnBackInvokedCallback;
import android.window.OnBackInvokedDispatcher;
import android.os.Build;

public class RsTerminalActivity extends NativeActivity {
    private boolean mBackCallbackRegistered = false;

    @Override
    protected void onStart() {
        super.onStart();
        if (!mBackCallbackRegistered && Build.VERSION.SDK_INT >= 33) {
            getOnBackInvokedDispatcher().registerOnBackInvokedCallback(
                OnBackInvokedDispatcher.PRIORITY_DEFAULT,
                () -> nativeOnBackPressed()
            );
            mBackCallbackRegistered = true;
        }
    }

    @Override
    public void onBackPressed() {
        // Fallback for older API levels (pre-33).
        nativeOnBackPressed();
    }

    private static native void nativeOnBackPressed();
}
