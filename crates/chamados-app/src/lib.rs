//! Serviços de aplicação do `chamados`, sem terminal: o que o CLI e os aplicativos móveis têm em
//! comum acima do núcleo (`chamados-core`, `chamados-sync`, `suap-core`).
//!
//! Nada aqui lê a entrada padrão, imprime ou pergunta algo: cada função recebe o que precisa e
//! devolve dados, para o CLI formatar em texto e os aplicativos mostrarem em telas.

use chamados_core::TicketError;
use thiserror::Error;

pub mod apply;
pub mod batch;
pub mod download;
pub mod files;
pub mod listing;
pub mod pairing;
pub mod pdf;
pub mod share;
pub mod syncing;
pub mod watching;

/// Falhas dos serviços: do SUAP (`Ticket`), do disco (`Io`) ou uma recusa já explicada em texto.
#[derive(Debug, Error)]
pub enum AppError {
    #[error(transparent)]
    Ticket(#[from] TicketError),
    #[error(transparent)]
    Io(#[from] std::io::Error),
    #[error("{0}")]
    Message(String),
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn errors_keep_the_text_of_their_source() {
        let ticket: AppError = TicketError::Source("fora do ar".to_owned()).into();
        assert_eq!(ticket.to_string(), "ticket source error: fora do ar");
        let io: AppError = std::io::Error::other("disco cheio").into();
        assert_eq!(io.to_string(), "disco cheio");
        assert_eq!(
            AppError::Message("não pode".to_owned()).to_string(),
            "não pode"
        );
    }
}
