plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

android {
    namespace = "br.edu.ifrn.chamados"
    compileSdk = 37

    defaultConfig {
        applicationId = "br.edu.ifrn.chamados"
        minSdk = 24
        targetSdk = 37
        versionCode = 1
        versionName = "0.36.0"
    }

    buildTypes {
        debug { }
    }
}

dependencies {
    // Telas em Jetpack Compose (Material 3); a BOM fixa as versões do conjunto.
    implementation(platform("androidx.compose:compose-bom:2025.09.01"))
    implementation("androidx.compose.material3:material3")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.activity:activity-compose:1.11.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.9.4")

    // O UniFFI do Kotlin chama a biblioteca Rust por JNA.
    implementation("net.java.dev.jna:jna:5.17.0@aar")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.10.2")

    // Ler o QR do pareamento da sincronização: câmera (CameraX) + leitor de códigos (ML Kit, com o
    // modelo embutido: funciona sem rede e sem os serviços do Google).
    implementation("androidx.camera:camera-core:1.4.2")
    implementation("androidx.camera:camera-camera2:1.4.2")
    implementation("androidx.camera:camera-lifecycle:1.4.2")
    implementation("androidx.camera:camera-view:1.4.2")
    implementation("com.google.mlkit:barcode-scanning:17.3.0")

    // Acompanhamento em segundo plano: agendamento periódico que sobrevive a reinício e respeita o Doze.
    implementation("androidx.work:work-runtime-ktx:2.10.0")
}
