# JNI resolves these private native methods by their class and method names.
-keep class com.routebible.recall.JniNativeBridge {
    native <methods>;
}

-keepclasseswithmembernames,includedescriptorclasses class * {
    native <methods>;
}

-keepattributes RuntimeVisibleAnnotations,AnnotationDefault
