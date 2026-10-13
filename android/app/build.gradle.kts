plugins {
    id("com.android.application")
}

android {
    namespace = "br.edu.ifrn.chamados"
    compileSdk = 37

    defaultConfig {
        applicationId = "br.edu.ifrn.chamados"
        minSdk = 24
        targetSdk = 37
        versionCode = 1
        versionName = "0.0.1-spike"
    }

    buildTypes {
        debug { }
    }
}

dependencies {
    // O UniFFI do Kotlin chama a biblioteca Rust por JNA.
    implementation("net.java.dev.jna:jna:5.17.0@aar")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")
}
