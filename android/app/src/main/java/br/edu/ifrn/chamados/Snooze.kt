package br.edu.ifrn.chamados

import androidx.activity.compose.rememberLauncherForActivityResult
import androidx.activity.result.contract.ActivityResultContracts
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.ExperimentalLayoutApi
import androidx.compose.foundation.layout.FlowRow
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.rememberScrollState
import androidx.compose.foundation.verticalScroll
import androidx.compose.material3.AlertDialog
import androidx.compose.material3.DatePicker
import androidx.compose.material3.DatePickerDialog
import androidx.compose.material3.ExperimentalMaterial3Api
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedButton
import androidx.compose.material3.Text
import androidx.compose.material3.TextButton
import androidx.compose.material3.TimePicker
import androidx.compose.material3.rememberDatePickerState
import androidx.compose.material3.rememberTimePickerState
import androidx.compose.runtime.Composable
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Modifier
import androidx.compose.ui.platform.LocalContext
import androidx.compose.ui.unit.dp
import java.text.SimpleDateFormat
import java.util.Calendar
import java.util.Date
import java.util.Locale
import java.util.TimeZone
import uniffi.chamados_mobile.SnoozePreset
import uniffi.chamados_mobile.snoozeAt

/** `qui 15/10 08:00`, no fuso do aparelho. */
fun formatMoment(millis: ULong): String =
    SimpleDateFormat("EEE dd/MM HH:mm", Locale("pt", "BR")).format(Date(millis.toLong()))

/** O fuso do aparelho em minutos a leste de UTC (o que a biblioteca Rust pede para calcular os atalhos). */
private fun deviceOffsetMinutes(): Int = TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 60_000

@OptIn(ExperimentalMaterial3Api::class)
@Composable
fun SnoozeDialog(onPick: (ULong) -> Unit, onDismiss: () -> Unit) {
    var custom by remember { mutableStateOf(false) }
    if (custom) {
        CustomMoment(onPick = onPick, onDismiss = onDismiss)
        return
    }
    val offset = deviceOffsetMinutes()
    val presets = listOf(
        "Mais tarde" to SnoozePreset.LATER,
        "Amanhã" to SnoozePreset.TOMORROW,
        "Fim de semana" to SnoozePreset.WEEKEND,
        "Próxima semana" to SnoozePreset.NEXT_WEEK,
    )
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("Adiar até quando?") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                presets.forEach { (label, preset) ->
                    val until = snoozeAt(preset, offset)
                    TextButton(onClick = { onPick(until) }, modifier = Modifier.fillMaxWidth()) {
                        Text("$label · ${formatMoment(until)}", modifier = Modifier.fillMaxWidth())
                    }
                }
                TextButton(onClick = { custom = true }, modifier = Modifier.fillMaxWidth()) {
                    Text("Escolher data e hora…", modifier = Modifier.fillMaxWidth())
                }
            }
        },
        confirmButton = {},
        dismissButton = { TextButton(onClick = onDismiss) { Text("Voltar") } },
    )
}

/** Dia e hora à escolha: primeiro a data, depois a hora (no fuso do aparelho). */
@OptIn(ExperimentalMaterial3Api::class)
@Composable
private fun CustomMoment(onPick: (ULong) -> Unit, onDismiss: () -> Unit) {
    val dates = rememberDatePickerState()
    val times = rememberTimePickerState(initialHour = 8, initialMinute = 0, is24Hour = true)
    var askingTime by remember { mutableStateOf(false) }
    fun picked(): ULong? {
        // O seletor de datas devolve a meia-noite UTC do dia escolhido: só o dia interessa.
        val day = dates.selectedDateMillis ?: return null
        val utc = Calendar.getInstance(TimeZone.getTimeZone("UTC")).apply { timeInMillis = day }
        val local = Calendar.getInstance().apply {
            clear()
            set(utc.get(Calendar.YEAR), utc.get(Calendar.MONTH), utc.get(Calendar.DAY_OF_MONTH), times.hour, times.minute)
        }
        return local.timeInMillis.takeIf { it > System.currentTimeMillis() }?.toULong()
    }
    if (!askingTime) {
        // O diálogo próprio do seletor de datas dá a largura que o calendário precisa.
        DatePickerDialog(
            onDismissRequest = onDismiss,
            confirmButton = {
                TextButton(onClick = { askingTime = true }, enabled = dates.selectedDateMillis != null) { Text("Continuar") }
            },
            dismissButton = { TextButton(onClick = onDismiss) { Text("Voltar") } },
        ) { DatePicker(state = dates) }
        return
    }
    AlertDialog(
        onDismissRequest = onDismiss,
        title = { Text("A que horas?") },
        text = {
            Column(Modifier.verticalScroll(rememberScrollState())) {
                TimePicker(state = times)
                if (picked() == null) Text("Esse momento já passou.", color = MaterialTheme.colorScheme.error)
            }
        },
        confirmButton = { TextButton(onClick = { picked()?.let(onPick) }, enabled = picked() != null) { Text("Adiar") } },
        dismissButton = { TextButton(onClick = { askingTime = false }) { Text("Voltar") } },
    )
}
