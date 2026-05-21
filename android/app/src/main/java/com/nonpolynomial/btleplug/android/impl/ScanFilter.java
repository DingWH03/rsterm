package com.nonpolynomial.btleplug.android.impl;

import java.util.Arrays;

public class ScanFilter {
    private final String[] uuids;

    public ScanFilter(String[] uuids) {
        if (uuids == null) {
            this.uuids = new String[0];
        } else {
            this.uuids = Arrays.copyOf(uuids, uuids.length);
        }
    }

    public String[] getUuids() {
        return Arrays.copyOf(this.uuids, this.uuids.length);
    }
}
