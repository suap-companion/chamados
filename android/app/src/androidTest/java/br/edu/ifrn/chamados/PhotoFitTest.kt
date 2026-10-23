package br.edu.ifrn.chamados

import android.graphics.Bitmap
import android.graphics.BitmapFactory
import androidx.test.core.app.ApplicationProvider
import java.io.File
import kotlin.random.Random
import org.junit.After
import org.junit.Assert.assertEquals
import org.junit.Assert.assertNotNull
import org.junit.Assert.assertTrue
import org.junit.Test

/** A redução do JPEG da câmera: foto pequena fica como está; foto grande vira um JPEG menor que o limite, ainda legível. */
class PhotoFitTest {
    private val dir = File(ApplicationProvider.getApplicationContext<android.app.Application>().cacheDir, "foto-teste")

    @After
    fun cleanUp() {
        dir.deleteRecursively()
    }

    private fun noisyJpeg(name: String, size: Int): File {
        val random = Random(1)
        val bitmap = Bitmap.createBitmap(size, size, Bitmap.Config.ARGB_8888)
        val pixels = IntArray(size * size) { 0xFF000000.toInt() or random.nextInt(0xFFFFFF) }
        bitmap.setPixels(pixels, 0, size, 0, 0, size, size)
        dir.mkdirs()
        return File(dir, name).apply { outputStream().use { bitmap.compress(Bitmap.CompressFormat.JPEG, 100, it) } }
    }

    @Test
    fun a_photo_under_the_limit_is_left_alone() {
        val file = noisyJpeg("pequena.jpg", 200)
        val before = file.readBytes()
        assertEquals(file, PhotoFile.fit(file))
        assertTrue(before.contentEquals(file.readBytes()))
    }

    @Test
    fun a_photo_over_the_limit_is_shrunk_below_it_and_still_decodes() {
        val file = noisyJpeg("grande.jpg", 1600)
        val limit = 300_000L
        assertTrue("a foto de teste precisa passar do limite", file.length() > limit)
        PhotoFile.fit(file, limit)
        assertTrue(file.length() <= limit)
        val options = BitmapFactory.Options().apply { inJustDecodeBounds = true }
        BitmapFactory.decodeFile(file.absolutePath, options)
        assertNotNull(options.outWidth)
        assertTrue(options.outWidth in 1..1600)
    }

    @Test
    fun an_unreadable_file_over_the_limit_comes_back_untouched() {
        dir.mkdirs()
        val file = File(dir, "lixo.jpg").apply { writeBytes(ByteArray(2000) { 7 }) }
        assertEquals(file, PhotoFile.fit(file, 1000))
        assertEquals(2000L, file.length())
    }
}
