# Rustrack

Tracks VDA5050 AGV fleets over MQTT and shows them live on a LIF layout.

- `backend` – the server: subscribes to MQTT, stores systems, serves the API and web UI
- `web` – the React frontend
- `simulator` – a simulated fleet with an embedded MQTT broker
- `shared` – types shared by the above
- `installer` – the Windows installer

## Building

The backend embeds the frontend, so build the frontend first. It needs Node,
[pnpm](https://pnpm.io) and [wasm-pack](https://rustwasm.github.io/wasm-pack/).

```sh
cd web && pnpm install && pnpm build && cd ..
cargo build --release -p backend
```

Without `web/dist` the backend still builds, with a warning, and serves only the
API. For frontend work, run `pnpm dev` in `web/`, which proxies `/api` to a
backend on port 3000.

## Running

```sh
backend --listen 127.0.0.1:8080 --data-dir ./data
```

| Option             | Default                 |                               |
| ------------------ | ----------------------- | ----------------------------- |
| `-l`, `--listen`   | `0.0.0.0:3000`          | Address to listen on          |
| `--data-dir`       | the working directory   | Where `runtimes_store.db` goes |

## Windows service

The installer copies the program to `Program Files\Rustrack`, then registers and
starts it as the `Rustrack` service, set to start automatically. Build it with
[NSIS](https://nsis.sourceforge.io):

```sh
makensis -DVERSION=0.1.0 -DEXE=target/release/backend.exe installer/rustrack.nsi
```

This produces `installer/rustrack-setup-0.1.0.exe`. It asks for the listen
address, or takes it unattended:

```bat
rustrack-setup-0.1.0.exe /S /LISTEN=127.0.0.1:8080
```

To manage the service without the installer, from an elevated prompt:

```bat
rustrack.exe service install --listen 127.0.0.1:8080
rustrack.exe service uninstall
```

The service keeps its database in `%ProgramData%\Rustrack` and its logs in
`%ProgramData%\Rustrack\logs`, unless installed with `--data-dir`. Uninstalling
keeps both. Windows Firewall isn't changed, so allow the port to reach the
service from other machines.
