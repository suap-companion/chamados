package br.edu.ifrn.chamados

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import android.graphics.Matrix
import android.media.ExifInterface
import java.io.File
import java.time.LocalDateTime
import java.time.format.DateTimeFormatter
import kotlin.math.ceil
import kotlin.math.sqrt

/** A foto tirada pela câmera para anexar: o nome, o tamanho (o SUAP aceita até 10 MB) e a redução do JPEG. */
object PhotoFile {
    /** O limite do SUAP é 10 MB; fica um pouco abaixo para sobrar folga no envio. */
    const val MAX_BYTES = 9_500_000L

    /** "foto-20261023-141500.jpg". */
    fun fileName(now: LocalDateTime): String = "foto-" + now.format(DateTimeFormatter.ofPattern("yyyyMMdd-HHmmss")) + ".jpg"

    /**
     * De quanto reduzir a imagem (potência de 2, como o `inSampleSize` do Android) para um JPEG de `bytes` caber em
     * `maxBytes`: o tamanho do arquivo acompanha a quantidade de pixels, então a escala linear é a raiz da razão.
     */
    fun sampleSizeFor(bytes: Long, maxBytes: Long): Int {
        if (bytes <= maxBytes) return 1
        val scale = sqrt(bytes.toDouble() / maxBytes)
        var sample = 1
        while (sample < ceil(scale)) sample *= 2
        return sample
    }

    /** Os graus que a orientação gravada pela câmera (EXIF) pede para a imagem ficar em pé. */
    fun rotationDegrees(orientation: Int): Float = when (orientation) {
        ExifInterface.ORIENTATION_ROTATE_90 -> 90f
        ExifInterface.ORIENTATION_ROTATE_180 -> 180f
        ExifInterface.ORIENTATION_ROTATE_270 -> 270f
        else -> 0f
    }

    /**
     * Garante que o JPEG cabe em `maxBytes`: se já cabe, não mexe; senão reduz a imagem (e a qualidade, se preciso),
     * mantendo a orientação, e regrava o mesmo arquivo. Devolve o arquivo (inalterado se não deu para ler).
     */
    fun fit(file: File, maxBytes: Long = MAX_BYTES): File {
        if (file.length() <= maxBytes) return file
        val rotation = rotationDegrees(
            runCatching { ExifInterface(file.absolutePath).getAttributeInt(ExifInterface.TAG_ORIENTATION, ExifInterface.ORIENTATION_NORMAL) }
                .getOrDefault(ExifInterface.ORIENTATION_NORMAL),
        )
        var sample = sampleSizeFor(file.length(), maxBytes)
        repeat(MAX_ROUNDS) {
            val options = BitmapFactory.Options().apply { inSampleSize = sample }
            val decoded = BitmapFactory.decodeFile(file.absolutePath, options) ?: return file
            val bitmap = if (rotation == 0f) decoded else rotated(decoded, rotation)
            for (quality in QUALITIES) {
                val bytes = java.io.ByteArrayOutputStream().also { bitmap.compress(Bitmap.CompressFormat.JPEG, quality, it) }.toByteArray()
                if (bytes.size <= maxBytes) {
                    file.writeBytes(bytes)
                    return file
                }
            }
            sample *= 2
        }
        return file
    }

    private fun rotated(bitmap: Bitmap, degrees: Float): Bitmap =
        Bitmap.createBitmap(bitmap, 0, 0, bitmap.width, bitmap.height, Matrix().apply { postRotate(degrees) }, true)

    private val QUALITIES = intArrayOf(85, 70, 55)
    private const val MAX_ROUNDS = 4
}

/** O que dizer quando a câmera não pode ser usada (e oferecer o arquivo no lugar), ou `null` se pode. */
fun cameraNote(hasCamera: Boolean, permissionDenied: Boolean): String? = when {
    !hasCamera -> "Este aparelho não tem câmera: use \"Escolher arquivos\" para anexar uma foto que já está nele."
    permissionDenied -> "Sem a permissão da câmera não dá para tirar a foto. Libere-a nas configurações do aparelho ou use \"Escolher arquivos\"."
    else -> null
}
