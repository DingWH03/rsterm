# Required by btleplug Android/droidplug and jni-utils-rs.
# These classes are loaded from Rust through JNI, so R8/ProGuard may not see Java-side references.
-keep class com.nonpolynomial.** { *; }
-keep class io.github.gedgygedgy.** { *; }
-keepclassmembers class com.nonpolynomial.** { native <methods>; }
-keepclassmembers class io.github.gedgygedgy.** { native <methods>; }
-dontwarn com.nonpolynomial.**
-dontwarn io.github.gedgygedgy.**
