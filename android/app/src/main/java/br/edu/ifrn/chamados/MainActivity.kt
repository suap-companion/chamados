package br.edu.ifrn.chamados

import android.os.Bundle
import androidx.activity.ComponentActivity
import androidx.activity.compose.setContent
import androidx.activity.enableEdgeToEdge
import androidx.activity.viewModels
import androidx.compose.material3.Surface

class MainActivity : ComponentActivity() {
    private val model: MainViewModel by viewModels()

    override fun onCreate(savedInstanceState: Bundle?) {
        super.onCreate(savedInstanceState)
        Assistant.removeLeftoverLocalModel(this)
        enableEdgeToEdge()
        setContent {
            ChamadosTheme {
                Surface { ChamadosApp(model) }
            }
        }
    }
}
