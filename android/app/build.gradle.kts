plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.plugin.compose")
}

// A versão do app é a do workspace Rust (Cargo.toml): uma fonte só. versionCode = maior*10000 + menor*100 + correção.
val (major, minor, patch) = rootProject.projectDir.resolve("../Cargo.toml").readLines()
    .firstNotNullOf { Regex("""^version\s*=\s*"(\d+)\.(\d+)\.(\d+)"""").find(it) }
    .destructured

android {
    namespace = "br.edu.ifrn.chamados"
    compileSdk = 37

    defaultConfig {
        applicationId = "br.edu.ifrn.chamados"
        minSdk = 26
        targetSdk = 37
        versionCode = major.toInt() * 10000 + minor.toInt() * 100 + patch.toInt()
        versionName = "$major.$minor.$patch"
        testInstrumentationRunner = "androidx.test.runner.AndroidJUnitRunner"
    }

    // A chave de assinatura nunca fica no repositório: vem de variáveis de ambiente (veja docs/android.md).
    // Sem elas o `assembleRelease` gera um APK sem assinatura, que não instala (e a release não o publica).
    signingConfigs {
        System.getenv("ANDROID_KEYSTORE_FILE")?.let { keystore ->
            create("release") {
                storeFile = file(keystore)
                storePassword = System.getenv("ANDROID_KEYSTORE_PASSWORD")
                keyAlias = System.getenv("ANDROID_KEY_ALIAS")
                keyPassword = System.getenv("ANDROID_KEY_PASSWORD")
            }
        }
    }

    buildTypes {
        debug { }
        release {
            // Sem R8/ProGuard: JNA e o UniFFI usam reflexão, e o ganho de tamanho viria do Rust, não do Kotlin.
            isMinifyEnabled = false
            signingConfigs.findByName("release")?.let { signingConfig = it }
        }
    }
}

dependencies {
    // Telas em Jetpack Compose (Material 3); a BOM fixa as versões do conjunto.
    implementation(platform("androidx.compose:compose-bom:2025.09.01"))
    implementation("androidx.compose.material3:material3")
    // Os ícones básicos (menu, busca, fechar...) do cabeçalho do gov.br; só o conjunto "core", pequeno.
    implementation("androidx.compose.material:material-icons-core")
    implementation("androidx.compose.ui:ui")
    implementation("androidx.activity:activity-compose:1.11.0")
    implementation("androidx.lifecycle:lifecycle-viewmodel-compose:2.9.4")

    // O UniFFI do Kotlin chama a biblioteca Rust por JNA.
    implementation("net.java.dev.jna:jna:5.17.0@aar")
    implementation("org.jetbrains.kotlinx:kotlinx-coroutines-android:1.11.0")

    // Ler o QR do pareamento da sincronização: câmera (CameraX) + leitor de códigos (ML Kit, com o
    // modelo embutido: funciona sem rede e sem os serviços do Google).
    implementation("androidx.camera:camera-core:1.4.2")
    implementation("androidx.camera:camera-camera2:1.4.2")
    implementation("androidx.camera:camera-lifecycle:1.4.2")
    implementation("androidx.camera:camera-view:1.4.2")
    implementation("com.google.mlkit:barcode-scanning:17.3.0")
    // IA do sistema (Gemini Nano via AICore): "Sugerir com IA" nos textos dos chamados. Beta; a biblioteca pede
    // minSdk 26, o mesmo do app.
    implementation("com.google.mlkit:genai-prompt:1.0.0-beta4")
    // Modelo local de reserva (opcional, baixado só com consentimento): LiteRT-LM (o MediaPipe LLM Inference está em manutenção).
    implementation("com.google.ai.edge.litertlm:litertlm-android:0.16.1")

    // Acompanhamento em segundo plano: agendamento periódico que sobrevive a reinício e respeita o Doze.
    implementation("androidx.work:work-runtime-ktx:2.10.0")

    // Testes unitários (JVM, sem aparelho): a lógica de tela que não depende da biblioteca nativa.
    testImplementation("junit:junit:4.13.2")

    // Testes de instrumentação (no emulador ou aparelho, com a biblioteca nativa de verdade): telas Compose e
    // o fluxo de documentos. Rodam com `./gradlew connectedDebugAndroidTest`.
    androidTestImplementation("androidx.test.ext:junit:1.3.0")
    androidTestImplementation("androidx.test:runner:1.7.0")
    androidTestImplementation(platform("androidx.compose:compose-bom:2025.09.01"))
    androidTestImplementation("androidx.compose.ui:ui-test-junit4")
    debugImplementation("androidx.compose.ui:ui-test-manifest")
}
