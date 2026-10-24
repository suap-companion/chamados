package br.edu.ifrn.chamados

import android.Manifest
import android.content.pm.PackageManager
import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.core.content.ContextCompat
import androidx.core.content.FileProvider
import java.io.File
import java.time.LocalDateTime

/**
 * O botão "Foto": a câmera só é pedida nesta hora; sem câmera ou sem permissão, o aviso oferece o botão de
 * arquivos. A foto tirada entra nos anexos do `model` (reduzida se passar do limite do SUAP).
 */
@Composable
fun PhotoButton(model: MainViewModel) {
    val context = LocalContext.current
    var photoFile by remember { mutableStateOf<File?>(null) }
    var cameraProblem by remember { mutableStateOf<String?>(null) }
    val camera = rememberLauncherForActivityResult(ActivityResultContracts.TakePicture()) { taken ->
        val file = photoFile
        if (taken && file != null) model.addPhoto(file) else file?.delete()
    }
    fun takePhoto() {
        val directory = File(context.cacheDir, "fotos").apply { mkdirs() }
        val file = File(directory, PhotoFile.fileName(LocalDateTime.now()))
        photoFile = file
        camera.launch(FileProvider.getUriForFile(context, "${context.packageName}.files", file))
    }
    val cameraPermission = rememberLauncherForActivityResult(ActivityResultContracts.RequestPermission()) { granted ->
        cameraProblem = cameraNote(hasCamera = true, permissionDenied = !granted)
        if (granted) takePhoto()
    }
    OutlinedButton(
        onClick = {
            val hasCamera = context.packageManager.hasSystemFeature(PackageManager.FEATURE_CAMERA_ANY)
            val granted = ContextCompat.checkSelfPermission(context, Manifest.permission.CAMERA) == PackageManager.PERMISSION_GRANTED
            cameraProblem = cameraNote(hasCamera, permissionDenied = false)
            if (hasCamera) if (granted) takePhoto() else cameraPermission.launch(Manifest.permission.CAMERA)
        },
        modifier = Modifier.fillMaxWidth(),
    ) { Text("Foto") }
    cameraProblem?.let { Text(it, color = MaterialTheme.colorScheme.error, style = MaterialTheme.typography.labelMedium) }
}
