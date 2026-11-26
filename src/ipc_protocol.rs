use std::time::Duration;

/// Dimensione massima del payload di un singolo frame IPC (16 byte).
pub const MAX_IPC_FRAME_SIZE: usize = 16;

/// Opcode dei comandi IPC in ingresso (esattamente 1 byte).
pub const IPC_CMD_GET_STATUS: u8 = 0x01;
pub const IPC_CMD_SUSPEND: u8 = 0x02;
pub const IPC_CMD_RELOAD_CONFIG: u8 = 0x03;
pub const IPC_CMD_RESUME: u8 = 0x04;

/// Byte di risposta GetStatus: successo.
pub const IPC_RESP_STATUS_SUCCESS: u8 = 0x00;

/// Byte di stato del servizio per la risposta GetStatus.
pub const IPC_STATE_ACTIVE: u8 = 0x00;
pub const IPC_STATE_PAUSED: u8 = 0x01;
pub const IPC_STATE_ERROR: u8 = 0x02;

/// Byte di esito per il comando Suspend.
pub const IPC_SUSPEND_SUCCESS_OR_PAUSED: u8 = 0x01;
pub const IPC_SUSPEND_REJECTED_ERROR_STATE: u8 = 0x02;

/// Byte di esito per il comando ReloadConfig.
pub const IPC_RELOAD_ACCEPTED: u8 = 0x01;

/// Byte di esito per il comando Resume.
pub const IPC_RESUME_SUCCESS_OR_ACTIVE: u8 = 0x01;
pub const IPC_RESUME_REJECTED_ERROR_STATE: u8 = 0x02;

/// Byte di risposta per errore di protocollo.
pub const IPC_RESP_PROTOCOL_ERROR: u8 = 0xFF;

/// Timeout predefinito per le operazioni I/O asincrone sulla named pipe (2 secondi).
pub const DEFAULT_IPC_IO_TIMEOUT: Duration = Duration::from_millis(2000);

/// Comandi IPC ammessi dall'interfaccia di automazione locale.
///
/// Modello di sicurezza restrittivo: sono consentiti unicamente comandi di gestione stato e
/// ricarica configurazione (`GetStatus` `0x01`, `Suspend` `0x02`, `ReloadConfig` `0x03`, `Resume` `0x04`).
/// Qualsiasi altro opcode o comando di esecuzione/input è categoricamente escluso.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IpcCommand {
    GetStatus,
    Suspend,
    ReloadConfig,
    Resume,
}

/// Risposte trasmesse dal server IPC al client connesso.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum IpcResponse {
    Status { state: u8, active_rules: u32 },
    Suspend { outcome: u8 },
    Reload { outcome: u8 },
    Resume { outcome: u8 },
    Error,
}

/// Errori del protocollo e della comunicazione IPC su Named Pipe.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum IpcProtocolError {
    #[error("Frame vuoto (lunghezza 0)")]
    Empty,
    #[error("Dimensione del frame eccessiva ({0} byte, max 16)")]
    TooLarge(usize),
    #[error("Lunghezza payload non valida ({0} byte, atteso esattamente 1 byte)")]
    BadLength(usize),
    #[error("Opcode comando IPC sconosciuto (0x{0:02X})")]
    UnknownOpcode(u8),
    #[error("Operazione I/O IPC scaduta per timeout")]
    IoTimeout,
    #[error("Disconnessione del client IPC")]
    ClientDisconnected,
    #[error("Arresto richiesto del server IPC")]
    ServerStopped,
    #[error("Errore del sottosistema Windows: {0}")]
    SystemError(String),
}

/// Genera il nome univoco della Named Pipe associato al SID dell'utente corrente.
pub fn get_pipe_name(sid: &str) -> String {
    format!(r"\\.\pipe\RustBind-IPC-{}", sid)
}

/// Codifica un comando IPC in un frame da 5 byte (4 byte lunghezza=1 LE + 1 byte opcode).
pub fn encode_ipc_command(cmd: IpcCommand) -> [u8; 5] {
    let opcode = match cmd {
        IpcCommand::GetStatus => IPC_CMD_GET_STATUS,
        IpcCommand::Suspend => IPC_CMD_SUSPEND,
        IpcCommand::ReloadConfig => IPC_CMD_RELOAD_CONFIG,
        IpcCommand::Resume => IPC_CMD_RESUME,
    };
    let len_bytes = 1u32.to_le_bytes();
    [
        len_bytes[0],
        len_bytes[1],
        len_bytes[2],
        len_bytes[3],
        opcode,
    ]
}

/// Valida l'intestazione a 4 byte (little-endian) che precede ogni frame comando IPC in ingresso.
///
/// Rifiuta:
/// - lunghezza 0 -> `IpcProtocolError::Empty`
/// - lunghezza > 16 -> `IpcProtocolError::TooLarge` (senza leggere il corpo)
/// - lunghezza != 1 -> `IpcProtocolError::BadLength`
pub fn parse_frame_header(header: &[u8; 4]) -> Result<usize, IpcProtocolError> {
    let len = u32::from_le_bytes(*header) as usize;
    if len == 0 {
        return Err(IpcProtocolError::Empty);
    }
    if len > MAX_IPC_FRAME_SIZE {
        return Err(IpcProtocolError::TooLarge(len));
    }
    if len != 1 {
        return Err(IpcProtocolError::BadLength(len));
    }
    Ok(len)
}

/// Valida l'intestazione a 4 byte (little-endian) che precede ogni risposta IPC lato client.
///
/// Accetta lunghezze comprese in `1..=MAX_IPC_FRAME_SIZE`:
/// - lunghezza 0 -> `IpcProtocolError::Empty`
/// - lunghezza > 16 -> `IpcProtocolError::TooLarge`
pub fn parse_response_header(header: &[u8; 4]) -> Result<usize, IpcProtocolError> {
    let len = u32::from_le_bytes(*header) as usize;
    if len == 0 {
        return Err(IpcProtocolError::Empty);
    }
    if len > MAX_IPC_FRAME_SIZE {
        return Err(IpcProtocolError::TooLarge(len));
    }
    Ok(len)
}

/// Decodifica il singolo byte di payload di un comando IPC.
pub fn parse_ipc_payload(payload: &[u8]) -> Result<IpcCommand, IpcProtocolError> {
    if payload.len() != 1 {
        return Err(IpcProtocolError::BadLength(payload.len()));
    }
    match payload[0] {
        IPC_CMD_GET_STATUS => Ok(IpcCommand::GetStatus),
        IPC_CMD_SUSPEND => Ok(IpcCommand::Suspend),
        IPC_CMD_RELOAD_CONFIG => Ok(IpcCommand::ReloadConfig),
        IPC_CMD_RESUME => Ok(IpcCommand::Resume),
        opcode => Err(IpcProtocolError::UnknownOpcode(opcode)),
    }
}

/// Valida e deserializza un payload grezzo di un frame IPC (funzione pura).
pub fn parse_ipc_frame(payload: &[u8]) -> Result<IpcCommand, IpcProtocolError> {
    if payload.is_empty() {
        return Err(IpcProtocolError::Empty);
    }
    if payload.len() > MAX_IPC_FRAME_SIZE {
        return Err(IpcProtocolError::TooLarge(payload.len()));
    }
    if payload.len() != 1 {
        return Err(IpcProtocolError::BadLength(payload.len()));
    }
    parse_ipc_payload(payload)
}

/// Codifica una risposta IPC serializzandola in byte con intestazione di lunghezza (4 byte LE).
pub fn encode_ipc_response(response: &IpcResponse) -> Vec<u8> {
    match response {
        IpcResponse::Status {
            state,
            active_rules,
        } => {
            let payload_len = 6u32;
            let mut bytes = Vec::with_capacity(10);
            bytes.extend_from_slice(&payload_len.to_le_bytes());
            bytes.push(IPC_RESP_STATUS_SUCCESS);
            bytes.push(*state);
            bytes.extend_from_slice(&active_rules.to_le_bytes());
            bytes
        }
        IpcResponse::Suspend { outcome } => {
            let payload_len = 1u32;
            let mut bytes = Vec::with_capacity(5);
            bytes.extend_from_slice(&payload_len.to_le_bytes());
            bytes.push(*outcome);
            bytes
        }
        IpcResponse::Reload { outcome } => {
            let payload_len = 2u32;
            let mut bytes = Vec::with_capacity(6);
            bytes.extend_from_slice(&payload_len.to_le_bytes());
            bytes.push(IPC_CMD_RELOAD_CONFIG);
            bytes.push(*outcome);
            bytes
        }
        IpcResponse::Resume { outcome } => {
            let payload_len = 2u32;
            let mut bytes = Vec::with_capacity(6);
            bytes.extend_from_slice(&payload_len.to_le_bytes());
            bytes.push(IPC_CMD_RESUME);
            bytes.push(*outcome);
            bytes
        }
        IpcResponse::Error => {
            let payload_len = 1u32;
            let mut bytes = Vec::with_capacity(5);
            bytes.extend_from_slice(&payload_len.to_le_bytes());
            bytes.push(IPC_RESP_PROTOCOL_ERROR);
            bytes
        }
    }
}

/// Decodifica il solo payload di una risposta IPC già delimitato in base all'header.
fn decode_ipc_response_payload(payload: &[u8]) -> Result<IpcResponse, IpcProtocolError> {
    if payload.len() == 1 {
        if payload[0] == IPC_RESP_PROTOCOL_ERROR {
            Ok(IpcResponse::Error)
        } else if payload[0] == IPC_SUSPEND_SUCCESS_OR_PAUSED
            || payload[0] == IPC_SUSPEND_REJECTED_ERROR_STATE
        {
            Ok(IpcResponse::Suspend {
                outcome: payload[0],
            })
        } else {
            Err(IpcProtocolError::UnknownOpcode(payload[0]))
        }
    } else if payload.len() == 2 {
        match payload[0] {
            IPC_CMD_RELOAD_CONFIG => {
                if payload[1] == IPC_RELOAD_ACCEPTED {
                    Ok(IpcResponse::Reload {
                        outcome: payload[1],
                    })
                } else {
                    Err(IpcProtocolError::UnknownOpcode(payload[1]))
                }
            }
            IPC_CMD_RESUME => {
                if payload[1] == IPC_RESUME_SUCCESS_OR_ACTIVE
                    || payload[1] == IPC_RESUME_REJECTED_ERROR_STATE
                {
                    Ok(IpcResponse::Resume {
                        outcome: payload[1],
                    })
                } else {
                    Err(IpcProtocolError::UnknownOpcode(payload[1]))
                }
            }
            other => Err(IpcProtocolError::UnknownOpcode(other)),
        }
    } else if payload.len() == 6 {
        if payload[0] != IPC_RESP_STATUS_SUCCESS {
            return Err(IpcProtocolError::UnknownOpcode(payload[0]));
        }
        let state = payload[1];
        let rules_bytes = [payload[2], payload[3], payload[4], payload[5]];
        let active_rules = u32::from_le_bytes(rules_bytes);
        Ok(IpcResponse::Status {
            state,
            active_rules,
        })
    } else {
        Err(IpcProtocolError::BadLength(payload.len()))
    }
}

/// Decodifica una risposta IPC completa (4 byte header LE + payload).
pub fn decode_ipc_response(bytes: &[u8]) -> Result<IpcResponse, IpcProtocolError> {
    if bytes.len() < 4 {
        return Err(IpcProtocolError::Empty);
    }
    let header = [bytes[0], bytes[1], bytes[2], bytes[3]];
    let len = u32::from_le_bytes(header) as usize;
    let payload = &bytes[4..];
    if payload.len() != len {
        return Err(IpcProtocolError::BadLength(payload.len()));
    }
    decode_ipc_response_payload(payload)
}

/// Verifica se la durata trascorsa per un'operazione ha superato il timeout configurato.
///
/// Funzione pura per test unitari e simulazione di timeout I/O.
pub fn validate_io_duration(elapsed: Duration, timeout: Duration) -> Result<(), IpcProtocolError> {
    if elapsed > timeout {
        Err(IpcProtocolError::IoTimeout)
    } else {
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_pipe_name() {
        assert_eq!(
            get_pipe_name("S-1-5-21-1001"),
            r"\\.\pipe\RustBind-IPC-S-1-5-21-1001"
        );
    }

    #[test]
    fn test_encode_ipc_command() {
        assert_eq!(
            encode_ipc_command(IpcCommand::GetStatus),
            [1, 0, 0, 0, IPC_CMD_GET_STATUS]
        );
        assert_eq!(
            encode_ipc_command(IpcCommand::Suspend),
            [1, 0, 0, 0, IPC_CMD_SUSPEND]
        );
    }

    #[test]
    fn test_parse_frame_header_valid() {
        let header = 1u32.to_le_bytes();
        let len = parse_frame_header(&header).expect("Header valido");
        assert_eq!(len, 1);
    }

    #[test]
    fn test_parse_frame_header_empty() {
        let header = 0u32.to_le_bytes();
        assert_eq!(parse_frame_header(&header), Err(IpcProtocolError::Empty));
    }

    #[test]
    fn test_parse_frame_header_too_large() {
        let header = 17u32.to_le_bytes();
        assert_eq!(
            parse_frame_header(&header),
            Err(IpcProtocolError::TooLarge(17))
        );
    }

    #[test]
    fn test_parse_frame_header_bad_length() {
        for invalid_len in [2u32, 5u32, 16u32] {
            let header = invalid_len.to_le_bytes();
            assert_eq!(
                parse_frame_header(&header),
                Err(IpcProtocolError::BadLength(invalid_len as usize))
            );
        }
    }

    #[test]
    fn test_parse_response_header_bounds() {
        assert_eq!(
            parse_response_header(&0u32.to_le_bytes()),
            Err(IpcProtocolError::Empty)
        );
        assert_eq!(parse_response_header(&1u32.to_le_bytes()), Ok(1));
        assert_eq!(parse_response_header(&6u32.to_le_bytes()), Ok(6));
        assert_eq!(
            parse_response_header(&(MAX_IPC_FRAME_SIZE as u32).to_le_bytes()),
            Ok(MAX_IPC_FRAME_SIZE)
        );
        assert_eq!(
            parse_response_header(&(MAX_IPC_FRAME_SIZE as u32 + 1).to_le_bytes()),
            Err(IpcProtocolError::TooLarge(MAX_IPC_FRAME_SIZE + 1))
        );
    }

    #[test]
    fn test_parse_ipc_payload_get_status() {
        let payload = [IPC_CMD_GET_STATUS];
        let cmd = parse_ipc_payload(&payload).expect("GetStatus valido");
        assert_eq!(cmd, IpcCommand::GetStatus);
    }

    #[test]
    fn test_parse_ipc_payload_suspend() {
        let payload = [IPC_CMD_SUSPEND];
        let cmd = parse_ipc_payload(&payload).expect("Suspend valido");
        assert_eq!(cmd, IpcCommand::Suspend);
    }

    #[test]
    fn test_parse_ipc_payload_reload_and_resume() {
        assert_eq!(
            parse_ipc_payload(&[IPC_CMD_RELOAD_CONFIG]),
            Ok(IpcCommand::ReloadConfig)
        );
        assert_eq!(parse_ipc_payload(&[IPC_CMD_RESUME]), Ok(IpcCommand::Resume));
        assert_eq!(
            encode_ipc_command(IpcCommand::ReloadConfig),
            [1, 0, 0, 0, IPC_CMD_RELOAD_CONFIG]
        );
        assert_eq!(
            encode_ipc_command(IpcCommand::Resume),
            [1, 0, 0, 0, IPC_CMD_RESUME]
        );
    }

    #[test]
    fn test_parse_ipc_payload_unknown_opcode() {
        for opcode in [0x00, 0x80, 0xFF] {
            assert_eq!(
                parse_ipc_payload(&[opcode]),
                Err(IpcProtocolError::UnknownOpcode(opcode))
            );
        }
    }

    #[test]
    fn test_parse_ipc_payload_forbidden_opcodes_range() {
        // Verifica che ogni opcode non autorizzato nel range 0x05..=0x7F venga categoricamente respinto
        for opcode in 0x05..=0x7F {
            assert_eq!(
                parse_ipc_payload(&[opcode]),
                Err(IpcProtocolError::UnknownOpcode(opcode)),
                "L'opcode 0x{:02X} doveva essere respinto",
                opcode
            );
        }
    }

    #[test]
    fn test_parse_ipc_frame_pure() {
        assert_eq!(parse_ipc_frame(&[]), Err(IpcProtocolError::Empty));
        assert_eq!(
            parse_ipc_frame(&[0u8; 17]),
            Err(IpcProtocolError::TooLarge(17))
        );
        assert_eq!(
            parse_ipc_frame(&[0x01, 0x02]),
            Err(IpcProtocolError::BadLength(2))
        );
        assert_eq!(
            parse_ipc_frame(&[IPC_CMD_GET_STATUS]),
            Ok(IpcCommand::GetStatus)
        );
        assert_eq!(parse_ipc_frame(&[IPC_CMD_SUSPEND]), Ok(IpcCommand::Suspend));
    }

    #[test]
    fn test_encode_decode_response_roundtrip() {
        // Status response: 10 byte (4 len + 1 resp + 1 stato + 4 active_rules)
        let resp_status = IpcResponse::Status {
            state: IPC_STATE_ACTIVE,
            active_rules: 42,
        };
        let encoded = encode_ipc_response(&resp_status);
        assert_eq!(encoded.len(), 10);
        assert_eq!(u32::from_le_bytes(encoded[..4].try_into().unwrap()), 6);
        assert_eq!(decode_ipc_response(&encoded), Ok(resp_status));

        // Suspend response (0x01): 5 byte (4 len + 1 outcome)
        let resp_suspend = IpcResponse::Suspend {
            outcome: IPC_SUSPEND_SUCCESS_OR_PAUSED,
        };
        let encoded = encode_ipc_response(&resp_suspend);
        assert_eq!(encoded.len(), 5);
        assert_eq!(u32::from_le_bytes(encoded[..4].try_into().unwrap()), 1);
        assert_eq!(decode_ipc_response(&encoded), Ok(resp_suspend));

        // Suspend response (0x02, rejected error state): 5 byte
        let resp_suspend_err = IpcResponse::Suspend {
            outcome: IPC_SUSPEND_REJECTED_ERROR_STATE,
        };
        let encoded = encode_ipc_response(&resp_suspend_err);
        assert_eq!(encoded.len(), 5);
        assert_eq!(decode_ipc_response(&encoded), Ok(resp_suspend_err));

        // Error response (0xFF): 5 byte
        let resp_err = IpcResponse::Error;
        let encoded = encode_ipc_response(&resp_err);
        assert_eq!(encoded.len(), 5);
        assert_eq!(encoded[4], IPC_RESP_PROTOCOL_ERROR);
        assert_eq!(decode_ipc_response(&encoded), Ok(resp_err));

        // Reload response: 6 byte (4 len=2 + [0x03, outcome])
        let resp_reload = IpcResponse::Reload {
            outcome: IPC_RELOAD_ACCEPTED,
        };
        let encoded = encode_ipc_response(&resp_reload);
        assert_eq!(encoded.len(), 6);
        assert_eq!(u32::from_le_bytes(encoded[..4].try_into().unwrap()), 2);
        assert_eq!(decode_ipc_response(&encoded), Ok(resp_reload));

        // Resume response (active/resumed & rejected error): 6 byte (4 len=2 + [0x04, outcome])
        let resp_resume_ok = IpcResponse::Resume {
            outcome: IPC_RESUME_SUCCESS_OR_ACTIVE,
        };
        let encoded = encode_ipc_response(&resp_resume_ok);
        assert_eq!(encoded.len(), 6);
        assert_eq!(decode_ipc_response(&encoded), Ok(resp_resume_ok));

        let resp_resume_err = IpcResponse::Resume {
            outcome: IPC_RESUME_REJECTED_ERROR_STATE,
        };
        let encoded = encode_ipc_response(&resp_resume_err);
        assert_eq!(encoded.len(), 6);
        assert_eq!(decode_ipc_response(&encoded), Ok(resp_resume_err));
    }

    #[test]
    fn test_simulated_io_timeout() {
        let limit = Duration::from_millis(50);
        let normal = Duration::from_millis(10);
        let expired = Duration::from_millis(100);

        assert_eq!(validate_io_duration(normal, limit), Ok(()));
        assert_eq!(
            validate_io_duration(expired, limit),
            Err(IpcProtocolError::IoTimeout)
        );
    }
}
