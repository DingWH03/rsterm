package dev.rsTerminal.app;

import android.app.NativeActivity;

public class RsTerminalActivity extends NativeActivity {
    @Override
    public void onBackPressed() {
        nativeOnBackPressed();
    }
    private static native void nativeOnBackPressed();
}
