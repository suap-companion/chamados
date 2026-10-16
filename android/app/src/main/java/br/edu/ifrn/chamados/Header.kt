package br.edu.ifrn.chamados

import androidx.compose.foundation.Canvas
import androidx.compose.foundation.background
import androidx.compose.foundation.clickable
import androidx.compose.foundation.layout.Arrangement
import androidx.compose.foundation.layout.Box
import androidx.compose.foundation.layout.Column
import androidx.compose.foundation.layout.Row
import androidx.compose.foundation.layout.Spacer
import androidx.compose.foundation.layout.fillMaxWidth
import androidx.compose.foundation.layout.height
import androidx.compose.foundation.layout.padding
import androidx.compose.foundation.layout.size
import androidx.compose.foundation.layout.statusBarsPadding
import androidx.compose.foundation.layout.width
import androidx.compose.foundation.shape.CircleShape
import androidx.compose.foundation.shape.RoundedCornerShape
import androidx.compose.foundation.text.KeyboardActions
import androidx.compose.foundation.text.KeyboardOptions
import androidx.compose.material.icons.Icons
import androidx.compose.material.icons.automirrored.filled.ArrowBack
import androidx.compose.material.icons.filled.ArrowDropDown
import androidx.compose.material.icons.filled.Check
import androidx.compose.material.icons.filled.Close
import androidx.compose.material.icons.filled.Menu
import androidx.compose.material.icons.filled.MoreVert
import androidx.compose.material.icons.filled.Person
import androidx.compose.material.icons.filled.Search
import androidx.compose.material3.DropdownMenu
import androidx.compose.material3.DropdownMenuItem
import androidx.compose.material3.HorizontalDivider
import androidx.compose.material3.Icon
import androidx.compose.material3.IconButton
import androidx.compose.material3.MaterialTheme
import androidx.compose.material3.OutlinedTextField
import androidx.compose.material3.Surface
import androidx.compose.material3.Text
import androidx.compose.runtime.Composable
import androidx.compose.runtime.LaunchedEffect
import androidx.compose.runtime.getValue
import androidx.compose.runtime.mutableStateOf
import androidx.compose.runtime.remember
import androidx.compose.runtime.setValue
import androidx.compose.ui.Alignment
import androidx.compose.ui.Modifier
import androidx.compose.ui.draw.clip
import androidx.compose.ui.focus.FocusRequester
import androidx.compose.ui.focus.focusRequester
import androidx.compose.ui.geometry.Offset
import androidx.compose.ui.graphics.Color
import androidx.compose.ui.graphics.vector.ImageVector
import androidx.compose.ui.platform.LocalConfiguration
import androidx.compose.ui.semantics.contentDescription
import androidx.compose.ui.semantics.semantics
import androidx.compose.ui.text.font.FontWeight
import androidx.compose.ui.text.input.ImeAction
import androidx.compose.ui.text.style.TextOverflow
import androidx.compose.ui.unit.dp
import androidx.compose.ui.unit.sp

/** Um item dos menus do cabeçalho (acesso rápido, funcionalidades, listas, conta). */
data class HeaderItem(val label: String, val selected: Boolean = false, val onClick: () -> Unit)

/** A busca do cabeçalho: o texto, o que fazer ao mudar e ao confirmar (Enter) e o que fazer ao fechar. */
data class HeaderSearch(
    val label: String,
    val value: String,
    val onChange: (String) -> Unit,
    val onSubmit: () -> Unit,
    val onClose: () -> Unit,
)

/**
 * Cabeçalho no padrão do Design System do gov.br (componente "Header"): logo, título (uma linha, com
 * reticências), menu (☰), acesso rápido (⋮), funcionalidades (grade), busca e conta (Entrar/avatar).
 * Nas telas de detalhe, `onBack` troca o ☰ por uma seta de voltar.
 */
@Composable
fun GovBrHeader(
    title: String,
    subtitle: String? = null,
    onBack: (() -> Unit)? = null,
    menu: List<HeaderItem> = emptyList(),
    menuTitle: String = "LISTAS",
    quick: List<HeaderItem> = emptyList(),
    functions: List<HeaderItem> = emptyList(),
    search: HeaderSearch? = null,
    account: List<HeaderItem>? = null,
    signIn: (() -> Unit)? = null,
) {
    var searching by remember { mutableStateOf(false) }
    var open by remember { mutableStateOf<String?>(null) }
    val wide = LocalConfiguration.current.screenWidthDp >= 600
    Surface(color = MaterialTheme.colorScheme.surface, shadowElevation = 4.dp, modifier = Modifier.fillMaxWidth()) {
        Column(Modifier.statusBarsPadding().padding(horizontal = 12.dp, vertical = 4.dp)) {
            if (searching && search != null) {
                SearchRow(search) {
                    searching = false
                    search.onClose()
                }
                return@Column
            }
            // Linha de cima: logo à esquerda; acesso rápido, funcionalidades, busca e conta à direita.
            Row(Modifier.fillMaxWidth().height(56.dp), verticalAlignment = Alignment.CenterVertically) {
                AppMark()
                Spacer(Modifier.weight(1f))
                if (quick.isNotEmpty()) {
                    Box {
                        HeaderIcon("Acesso rápido", { open = "quick" }) { Icon(Icons.Filled.MoreVert, null, tint = MaterialTheme.colorScheme.primary) }
                        HeaderMenu(open == "quick", "ACESSO RÁPIDO", quick) { open = null }
                    }
                }
                if (functions.isNotEmpty()) {
                    if (quick.isNotEmpty()) Divider()
                    Box {
                        HeaderIcon("Funcionalidades", { open = "functions" }) { GridIcon(MaterialTheme.colorScheme.primary) }
                        HeaderMenu(open == "functions", "FUNCIONALIDADES", functions) { open = null }
                    }
                }
                if (search != null) {
                    if (quick.isNotEmpty() || functions.isNotEmpty()) Divider()
                    HeaderIcon("Buscar", { searching = true }) { Icon(Icons.Filled.Search, null, tint = MaterialTheme.colorScheme.primary) }
                }
                when {
                    account != null -> Box {
                        Row(
                            Modifier.clip(RoundedCornerShape(24.dp)).clickable { open = "account" }.height(48.dp)
                                .semantics { contentDescription = "Conta" }.padding(start = 8.dp),
                            verticalAlignment = Alignment.CenterVertically,
                        ) {
                            Avatar()
                            Icon(Icons.Filled.ArrowDropDown, null, tint = MaterialTheme.colorScheme.primary)
                        }
                        HeaderMenu(open == "account", "CONTA", account) { open = null }
                    }
                    signIn != null -> SignInButton(signIn)
                }
            }
            // Linha de baixo: ☰ (ou voltar) e o título; o subtítulo só nas telas largas.
            Row(Modifier.fillMaxWidth().padding(bottom = 8.dp), verticalAlignment = Alignment.CenterVertically) {
                when {
                    onBack != null -> HeaderIcon("Voltar", onBack) { Icon(Icons.AutoMirrored.Filled.ArrowBack, null, tint = MaterialTheme.colorScheme.primary) }
                    menu.isNotEmpty() -> Box {
                        HeaderIcon("Menu", { open = "menu" }) { Icon(Icons.Filled.Menu, null, tint = MaterialTheme.colorScheme.primary) }
                        HeaderMenu(open == "menu", menuTitle, menu) { open = null }
                    }
                    else -> Spacer(Modifier.width(8.dp))
                }
                Column(Modifier.weight(1f).padding(start = 4.dp)) {
                    Text(title, style = MaterialTheme.typography.titleLarge.copy(fontWeight = FontWeight.Normal), maxLines = 1, overflow = TextOverflow.Ellipsis)
                    if (wide && subtitle != null) {
                        Text(subtitle, style = MaterialTheme.typography.bodySmall, maxLines = 1, overflow = TextOverflow.Ellipsis)
                    }
                }
            }
        }
    }
}

@Composable
private fun SearchRow(search: HeaderSearch, onClose: () -> Unit) {
    // O foco já vai para o campo, como no Design System.
    val focus = remember { FocusRequester() }
    LaunchedEffect(Unit) { focus.requestFocus() }
    Row(Modifier.fillMaxWidth().height(72.dp), verticalAlignment = Alignment.CenterVertically) {
        OutlinedTextField(
            value = search.value,
            onValueChange = search.onChange,
            label = { Text(search.label) },
            singleLine = true,
            trailingIcon = {
                IconButton(onClick = search.onSubmit) {
                    Icon(Icons.Filled.Search, contentDescription = "Buscar", tint = MaterialTheme.colorScheme.primary)
                }
            },
            keyboardActions = KeyboardActions(onSearch = { search.onSubmit() }),
            keyboardOptions = KeyboardOptions(imeAction = ImeAction.Search),
            modifier = Modifier.weight(1f).focusRequester(focus),
        )
        HeaderIcon("Fechar a busca", onClose) { Icon(Icons.Filled.Close, null, tint = MaterialTheme.colorScheme.primary) }
    }
}

@Composable
private fun HeaderIcon(description: String, onClick: () -> Unit, icon: @Composable () -> Unit) {
    IconButton(onClick = onClick, modifier = Modifier.semantics { contentDescription = description }) { icon() }
}

@Composable
private fun Divider() {
    Box(Modifier.padding(horizontal = 2.dp).width(1.dp).height(28.dp).background(MaterialTheme.colorScheme.outlineVariant))
}

/** O menu suspenso do cabeçalho: título em caixa alta e itens de 48 dp, como no Design System. */
@Composable
private fun HeaderMenu(expanded: Boolean, title: String, items: List<HeaderItem>, onDismiss: () -> Unit) {
    DropdownMenu(expanded = expanded, onDismissRequest = onDismiss) {
        Text(
            title,
            style = MaterialTheme.typography.labelSmall.copy(fontWeight = FontWeight.Bold, letterSpacing = 0.5.sp),
            modifier = Modifier.padding(horizontal = 16.dp, vertical = 8.dp),
        )
        HorizontalDivider()
        items.forEach { item ->
            DropdownMenuItem(
                text = { Text(item.label, color = if (item.selected) MaterialTheme.colorScheme.primary else Color.Unspecified) },
                trailingIcon = if (item.selected) ({ Icon(Icons.Filled.Check, null, tint = MaterialTheme.colorScheme.primary) }) else null,
                onClick = {
                    onDismiss()
                    item.onClick()
                },
            )
        }
    }
}

/** Botão "Entrar" em pílula (como no DS). */
@Composable
private fun SignInButton(onClick: () -> Unit) {
    Row(
        Modifier.height(40.dp).clip(RoundedCornerShape(20.dp)).background(MaterialTheme.colorScheme.surfaceContainer)
            .clickable(onClick = onClick).padding(horizontal = 14.dp),
        verticalAlignment = Alignment.CenterVertically,
        horizontalArrangement = Arrangement.spacedBy(6.dp),
    ) {
        Icon(Icons.Filled.Person, null, tint = MaterialTheme.colorScheme.primary, modifier = Modifier.size(18.dp))
        Text("Entrar", color = MaterialTheme.colorScheme.primary, fontWeight = FontWeight.Bold, style = MaterialTheme.typography.labelLarge)
    }
}

@Composable
private fun Avatar() {
    Box(Modifier.size(36.dp).clip(CircleShape).background(MaterialTheme.colorScheme.primaryContainer), contentAlignment = Alignment.Center) {
        Icon(Icons.Filled.Person, null, tint = MaterialTheme.colorScheme.onPrimaryContainer)
    }
}

/** O logo do app no cabeçalho (uma marca simples; o logo de verdade entra aqui). */
@Composable
private fun AppMark() {
    Box(
        Modifier.size(36.dp).clip(RoundedCornerShape(8.dp)).background(MaterialTheme.colorScheme.primary)
            .semantics { contentDescription = "Chamados" },
        contentAlignment = Alignment.Center,
    ) {
        Text("C", color = MaterialTheme.colorScheme.onPrimary, fontWeight = FontWeight.Bold, fontSize = 20.sp)
    }
}

/** O ícone de "funcionalidades" do DS: grade de 3 × 3 pontos. */
@Composable
private fun GridIcon(color: Color) {
    Canvas(Modifier.size(22.dp)) {
        val step = size.width / 3
        for (row in 0..2) for (column in 0..2) {
            drawRect(color, topLeft = Offset(column * step + step * 0.12f, row * step + step * 0.12f), size = androidx.compose.ui.geometry.Size(step * 0.76f, step * 0.76f))
        }
    }
}
