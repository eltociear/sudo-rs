use std::{ffi::c_int, io, os::unix::process::ExitStatusExt, process::ExitStatus};

use signal_hook::{
    consts::*,
    iterator::{exfiltrator::WithOrigin, SignalsInfo},
    low_level::{
        emulate_default_handler,
        siginfo::{Cause, Origin, Process, Sent},
    },
};
use sudo_log::user_error;
use sudo_system::{close, getpgid, interface::ProcessId, kill, read};

pub(super) struct PtyRelay {
    signals: SignalsInfo<WithOrigin>,
    monitor_pid: ProcessId,
    sudo_pid: ProcessId,
    command_pid: ProcessId,
    // FIXME: Look for `SFD_LEADER` occurences in `exec_pty` to decide what to do with the leader
    // side of the pty. It should be used to handle signals like `SIGWINCH` and `SIGCONT`.
    pty_leader: c_int,
    rx: c_int,
    buf: [u8; std::mem::size_of::<i32>()],
}

impl PtyRelay {
    pub(super) fn new(
        monitor_pid: ProcessId,
        sudo_pid: ProcessId,
        pty_leader: c_int,
        rx: c_int,
    ) -> io::Result<Self> {
        Ok(Self {
            signals: SignalsInfo::<WithOrigin>::new(super::SIGNALS)?,
            monitor_pid,
            sudo_pid,
            // FIXME: is this ok? Check ogsudo's code.
            command_pid: -1,
            pty_leader,
            rx,
            buf: 0i32.to_ne_bytes(),
        })
    }

    pub(super) fn run(mut self) -> io::Result<ExitStatus> {
        loop {
            // First we check if the monitor sent us the exit status of the command.
            if let Some(status) = self.wait_monitor()? {
                return Ok(status);
            }

            // Then we check any pending signals that we received. Based on `signal_cb_pty`
            for info in self.signals.pending() {
                self.relay_signal(info);
            }
        }
    }

    fn wait_monitor(&mut self) -> io::Result<Option<ExitStatus>> {
        if read(self.rx, &mut self.buf).is_ok() {
            close(self.rx)?;

            let status = ExitStatus::from_raw(i32::from_ne_bytes(self.buf));

            if let Some(signal) = status.signal() {
                // If the command terminated because of a signal, we send this signal to sudo
                // itself to match the original sudo behavior. If we fail we just return the status
                // code.
                if kill(self.sudo_pid, signal) != -1 {
                    // Given that we overwrote the default handlers for all the signals, we musti
                    // emulate them to handle the signal we just sent correctly.
                    for info in self.signals.pending() {
                        emulate_default_handler(info.signal)?;
                    }
                }
            }

            close(self.pty_leader)?;
            Ok(Some(status))
        } else {
            Ok(None)
        }
    }

    fn relay_signal(&self, info: Origin) {
        let user_signaled = info.cause == Cause::Sent(Sent::User);
        match info.signal {
            // FIXME: check `handle_sigchld_pty`
            SIGCHLD => {}
            // FIXME: check `resume_terminal`
            SIGCONT => {}
            // FIXME: check `sync_ttysize`
            SIGWINCH => {}
            // Skip the signal if it was sent by the user and it is self-terminating.
            _ if user_signaled && self.is_self_terminating(info.process) => {}
            // FIXME: check `send_command_status`
            signal => {
                kill(self.monitor_pid, signal);
            }
        }
    }

    /// Decides if the signal sent by the `signaler` process is self-terminating.
    ///
    /// A signal is self-terminating if the PID of the `process`:
    /// - is the same PID of the command, or
    /// - is in the process group of the command and either sudo or the command is the leader.
    fn is_self_terminating(&self, signaler: Option<Process>) -> bool {
        if let Some(signaler) = signaler {
            if signaler.pid != 0 {
                if signaler.pid == self.command_pid {
                    return true;
                }

                let signaler_pgrp = getpgid(signaler.pid);
                if signaler_pgrp != -1 {
                    if signaler_pgrp == self.command_pid || signaler_pgrp == self.sudo_pid {
                        return true;
                    }
                } else {
                    user_error!("Could not fetch process group ID");
                }
            }
        }

        false
    }
}
