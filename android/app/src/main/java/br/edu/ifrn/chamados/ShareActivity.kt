package br.edu.ifrn.chamados

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.material3.Surface

/** Recebe o que o Android entrega por "Compartilhar" e pergunta o que fazer com isso. */
class ShareActivity : ComponentActivity() {
    private val model: ShareViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        enableEdgeToEdge()
        model.receive(intent)
        setContent {
            ChamadosTheme {
                Surface { ShareScreen(model) }
            }
        }
    }
}
