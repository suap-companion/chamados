package br.edu.ifrn.chamados

import android.Manifest
import android.content.Context
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.camera.core.CameraSelector
import androidx.camera.core.ExperimentalGetImage
import androidx.camera.core.ImageAnalysis
import androidx.camera.core.Preview
import androidx.camera.lifecycle.ProcessCameraProvider
import androidx.camera.view.PreviewView
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.fillMaxSize
import androidx.compose.foundation.layout.padding
import androidx.compose.material3.Button
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.DisposableEffect
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import androidx.compose.ui.viewinterop.AndroidView
import androidx.core.content.ContextCompat
import androidx.lifecycle.LifecycleOwner
import com.google.mlkit.vision.barcode.BarcodeScannerOptions
import com.google.mlkit.vision.barcode.BarcodeScanning
import com.google.mlkit.vision.barcode.common.Barcode
import com.google.mlkit.vision.common.InputImage
import java.util.concurrent.Executors
import java.util.concurrent.atomic.AtomicBoolean

/** Lê um QR com a câmera de trás e entrega o texto dele; a permissão da câmera é pedida na hora. */
@Composable
fun QrScanner(onResult: (String) -> Unit, onCancel: () -> Unit) {
    val context = LocalContext.current
    var granted by remember {
        mutableStateOf(ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED)
    }
    var refused by remember { mutableStateOf(false) }
    var failure by remember { mutableStateOf<String?>(null) }
    val request = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { ok ->
        granted = ok
        refused = !ok
    }
    LaunchedEffect(Unit) { if (!granted) request.launch(Manifest.permission.CAMERA) }
    Box(Modifier.fillMaxSize()) {
        if (granted && failure == null) {
            CameraPreview(onResult) { failure = it }
        } else {
            Column(
                Modifier.align(Alignment.Center).padding(24.dp),
                verticalArrangement = Arrangement.spacedBy(12.dp),
            ) {
                val text = when {
                    failure != null -> "Não foi possível abrir a câmera (${failure}). Cole o texto do pareamento na tela anterior."
                    refused -> "Sem a câmera não dá para ler o QR. Cole o texto do pareamento na tela anterior."
                    else -> "Aguardando a permissão da câmera…"
                }
                Text(text)
            }
        }
        Button(onClick = onCancel, modifier = Modifier.align(Alignment.BottomCenter).padding(24.dp)) { Text("Cancelar") }
    }
}

@OptIn(ExperimentalGetImage::class)
@Composable
private fun CameraPreview(onResult: (String) -> Unit, onError: (String) -> Unit) {
    val executor = remember { Executors.newSingleThreadExecutor() }
    val found = remember { AtomicBoolean(false) }
    val scanner = remember {
        BarcodeScanning.getClient(BarcodeScannerOptions.Builder().setBarcodeFormats(Barcode.FORMAT_QR_CODE).build())
    }
    DisposableEffect(Unit) {
        onDispose {
            executor.shutdown()
            scanner.close()
        }
    }
    AndroidView(
        modifier = Modifier.fillMaxSize(),
        factory = { context ->
            val view = PreviewView(context)
            val future = ProcessCameraProvider.getInstance(context)
            future.addListener({
                try {
                    bind(context, future.get(), view, executor, scanner, found, onResult)
                } catch (error: Throwable) {
                    onError(error.message ?: error.toString())
                }
            }, ContextCompat.getMainExecutor(context))
            view
        },
    )
}

@OptIn(ExperimentalGetImage::class)
private fun bind(
    context: Context,
    provider: ProcessCameraProvider,
    view: PreviewView,
    executor: java.util.concurrent.Executor,
    scanner: com.google.mlkit.vision.barcode.BarcodeScanner,
    found: AtomicBoolean,
    onResult: (String) -> Unit,
) {
    val preview = Preview.Builder().build().also { it.surfaceProvider = view.surfaceProvider }
    val analysis = ImageAnalysis.Builder()
        .setBackpressureStrategy(ImageAnalysis.STRATEGY_KEEP_ONLY_LATEST)
        .build()
    analysis.setAnalyzer(executor) { frame ->
        val media = frame.image
        if (media == null) {
            frame.close()
            return@setAnalyzer
        }
        val image = InputImage.fromMediaImage(media, frame.imageInfo.rotationDegrees)
        scanner.process(image)
            .addOnSuccessListener { codes ->
                val text = codes.firstNotNullOfOrNull { it.rawValue }
                if (text != null && found.compareAndSet(false, true)) onResult(text)
            }
            .addOnCompleteListener { frame.close() }
    }
    provider.unbindAll()
    provider.bindToLifecycle(context as LifecycleOwner, CameraSelector.DEFAULT_BACK_CAMERA, preview, analysis)
}
