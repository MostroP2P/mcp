#!/usr/bin/env bash
#
# Local Mostro regtest network for mostro-mcp.
#
#   ./devenv.sh up                 start everything and fund/open channels (idempotent)
#   ./devenv.sh status             containers, block height, channels
#   ./devenv.sh env <alice|bob>    export lines to point mostro-cli / mostro-mcp here
#   ./devenv.sh cli <alice|bob> …  run mostro-cli as that test user
#   ./devenv.sh invoice <alice|bob> <sats>   print a bolt11 from that user's LND
#   ./devenv.sh pay <alice|bob> <bolt11>     pay it (detached: hold invoices block)
#   ./devenv.sh lncli <mostro|alice|bob> …   raw lncli
#   ./devenv.sh mine [n]           mine n blocks (default 1)
#   ./devenv.sh logs [service]     follow logs
#   ./devenv.sh down               stop, keep state
#   ./devenv.sh clean              stop and wipe chain, wallets, relay, node db, CLI identities
#
# Regtest only: every key and coin here is a throwaway fixture.

set -Eeuo pipefail

readonly HERE="$(cd "$(dirname "$(realpath "${BASH_SOURCE[0]}")")" && pwd)"
readonly STATE_DIR="$HERE/.state"

# The node's public key (the mostro-regtest test key, see mostrod/settings.toml).
readonly MOSTRO_PUBKEY="724497f34e6c4f55f9aa90f5e9dcc326c76a9f2ed817b0f0eb940ee7218ac8d5"
readonly RELAY_PORT="${MOSTRO_RELAY_PORT:-7000}"
readonly RELAYS="ws://localhost:${RELAY_PORT}"

readonly CHANNEL_SATS=5000000
readonly PUSH_SATS=0

info() { printf '\033[1;34m==>\033[0m %s\n' "$*" >&2; }
fatal() { printf '\033[1;31mxx\033[0m %s\n' "$*" >&2; exit 1; }

dc() { docker compose -f "$HERE/compose.yml" "$@"; }

btc() {
  dc exec -T bitcoin bitcoin-cli -regtest -rpcport=18443 -rpcuser=bitcoin -rpcpassword=bitcoin "$@"
}

ln_service() {
  case "${1-}" in
    mostro) echo mostro_lnd ;;
    alice) echo alice_lnd ;;
    bob) echo bob_lnd ;;
    *) fatal "unknown LND node '${1-}' (mostro|alice|bob)" ;;
  esac
}

lncli() {
  local service
  service="$(ln_service "$1")"
  shift
  dc exec -T "$service" lncli --network=regtest --lnddir=/home/lnd/.lnd --rpcserver=localhost:10009 "$@"
}

mostro_cli_path() {
  if [[ -n "${MOSTRO_CLI_PATH-}" ]]; then
    echo "$MOSTRO_CLI_PATH"
  elif [[ -x "$HOME/dev/mostro-cli/target/release/mostro-cli" ]]; then
    echo "$HOME/dev/mostro-cli/target/release/mostro-cli"
  else
    command -v mostro-cli || fatal "mostro-cli not found: set MOSTRO_CLI_PATH"
  fi
}

user_dir() {
  case "${1-}" in
    alice | bob) echo "$STATE_DIR/$1" ;;
    *) fatal "unknown test user '${1-}' (alice|bob)" ;;
  esac
}

mine() {
  local n="${1:-1}" address
  address="$(btc -rpcwallet=dev getnewaddress)"
  btc generatetoaddress "$n" "$address" > /dev/null
}

wait_healthy() {
  local service="$1" tries=0
  until [[ "$(docker inspect -f '{{.State.Health.Status}}' "$(dc ps -q "$service")" 2> /dev/null)" == healthy ]]; do
    ((++tries > 90)) && fatal "$service did not become healthy"
    sleep 2
  done
}

wait_synced() {
  local node="$1" tries=0
  until [[ "$(lncli "$node" getinfo 2> /dev/null | jq -r .synced_to_chain)" == true ]]; do
    ((++tries > 90)) && fatal "$node LND did not sync to chain"
    sleep 2
  done
}

node_pubkey() { lncli "$1" getinfo | jq -r .identity_pubkey; }

active_channels() { lncli "$1" listchannels | jq '[.channels[] | select(.active)] | length'; }

init_chain() {
  wait_healthy bitcoin
  btc -rpcwallet=dev getwalletinfo &> /dev/null \
    || btc createwallet dev > /dev/null 2>&1 \
    || btc loadwallet dev > /dev/null
  if (($(btc getblockcount) < 101)); then
    info "mining 101 blocks (coinbase maturity)"
    mine 101
  fi
}

fund_and_open_channels() {
  local node
  for node in mostro alice bob; do
    wait_healthy "$(ln_service $node)"
    wait_synced "$node"
  done
  if (($(active_channels mostro) >= 4)); then
    info "channels already open"
    return
  fi

  # One UTXO per channel a node opens: a funding tx spends the wallet's
  # output and its change stays unconfirmed until the next block.
  if (($(confirmed_sats mostro) == 0)); then
    info "funding the three LND wallets"
    for node in mostro mostro alice bob; do
      btc -rpcwallet=dev sendtoaddress "$(lncli "$node" newaddress p2wkh | jq -r .address)" 5 > /dev/null
    done
    mine 6
  fi
  for node in mostro alice bob; do wait_funded "$node"; done

  # Mostro pays buyers (mostro → user) and users pay hold invoices (user → mostro).
  local mostro_pk alice_pk bob_pk
  mostro_pk="$(node_pubkey mostro)"
  alice_pk="$(node_pubkey alice)"
  bob_pk="$(node_pubkey bob)"
  lncli alice connect "${mostro_pk}@mostro_lnd:9735" &> /dev/null || true
  lncli bob connect "${mostro_pk}@mostro_lnd:9735" &> /dev/null || true

  info "opening channels mostro→alice, mostro→bob, alice→mostro, bob→mostro"
  open_channel mostro "$alice_pk"
  open_channel mostro "$bob_pk"
  open_channel alice "$mostro_pk"
  open_channel bob "$mostro_pk"
  mine 6

  local tries=0
  until (($(active_channels mostro) == 4)); do
    ((++tries > 60)) && fatal "channels did not become active"
    sleep 2
  done
  info "4 channels active"
}

confirmed_sats() { lncli "$1" walletbalance | jq -r '.confirmed_balance | tonumber'; }

wait_funded() {
  local node="$1" tries=0
  wait_synced "$node"
  until (($(confirmed_sats "$node") > CHANNEL_SATS)); do
    ((++tries > 60)) && fatal "$node has no confirmed funds"
    sleep 2
  done
}

# Opens a channel from <node> to <peer pubkey> unless one (open or pending,
# initiated by <node>) already exists.
open_channel() {
  local node="$1" peer="$2" open pending
  open="$(lncli "$node" listchannels \
    | jq --arg p "$peer" '[.channels[] | select(.remote_pubkey == $p and .initiator)] | length')"
  pending="$(lncli "$node" pendingchannels \
    | jq --arg p "$peer" '[.pending_open_channels[] | select(.channel.remote_node_pub == $p and .channel.initiator == "INITIATOR_LOCAL")] | length')"
  ((open + pending > 0)) && return
  lncli "$node" openchannel --node_key "$peer" --local_amt "$CHANNEL_SATS" --push_amt "$PUSH_SATS" > /dev/null
}

cmd_up() {
  info "starting the regtest network"
  dc up -d --build
  init_chain
  fund_and_open_channels
  mkdir -p "$STATE_DIR/alice" "$STATE_DIR/bob"
  chmod 700 "$STATE_DIR" "$STATE_DIR/alice" "$STATE_DIR/bob"
  info "ready — mostrod pubkey $MOSTRO_PUBKEY, relay $RELAYS"
  info "try: $0 cli alice listorders"
}

cmd_status() {
  dc ps
  printf '\nblock height: %s\n' "$(btc getblockcount)"
  local node
  for node in mostro alice bob; do
    printf '%-7s active channels: %s\n' "$node" "$(active_channels "$node" 2> /dev/null || echo '?')"
  done
}

cmd_env() {
  local dir
  dir="$(user_dir "${1-}")"
  cat <<EOF
export MOSTRO_CLI_PATH="$(mostro_cli_path)"
export MOSTRO_PUBKEY="$MOSTRO_PUBKEY"
export RELAYS="$RELAYS"
export MCLI_DIR="$dir"
EOF
}

cmd_cli() {
  local who="${1-}" dir
  dir="$(user_dir "$who")"
  shift
  mkdir -p "$dir" && chmod 700 "$dir"
  env MCLI_DIR="$dir" MOSTRO_PUBKEY="$MOSTRO_PUBKEY" RELAYS="$RELAYS" "$(mostro_cli_path)" "$@"
}

cmd_invoice() {
  local who="${1-}" sats="${2-}"
  [[ "$sats" =~ ^[0-9]+$ ]] || fatal "usage: invoice <alice|bob> <sats>"
  # mostrod rejects an invoice that expires before now + invoice_expiration_window
  # (3600 s in mostrod/settings.toml): give it two hours.
  lncli "$who" addinvoice --amt "$sats" --expiry 7200 | jq -r .payment_request
}

cmd_pay() {
  local who="${1-}" bolt11="${2-}" service
  [[ -n "$bolt11" ]] || fatal "usage: pay <alice|bob> <bolt11>"
  service="$(ln_service "$who")"
  # Detached: paying a hold invoice blocks until Mostro settles or cancels it.
  dc exec -d "$service" lncli --network=regtest --lnddir=/home/lnd/.lnd --rpcserver=localhost:10009 \
    payinvoice --force --pay_req "$bolt11"
  info "$who is paying (detached); check with: $0 lncli $who listpayments"
}

cmd_clean() {
  info "removing containers, volumes and test identities"
  dc down -v --remove-orphans
  rm -rf "$STATE_DIR"
}

main() {
  local cmd="${1-help}"
  shift || true
  case "$cmd" in
    up) cmd_up ;;
    status) cmd_status ;;
    env) cmd_env "$@" ;;
    cli) cmd_cli "$@" ;;
    invoice) cmd_invoice "$@" ;;
    pay) cmd_pay "$@" ;;
    lncli) lncli "$@" ;;
    btc) btc "$@" ;;
    mine) mine "${1:-1}" ;;
    logs) dc logs -f "$@" ;;
    down) dc down ;;
    clean) cmd_clean ;;
    *) sed -n '3,17p' "${BASH_SOURCE[0]}" | sed 's/^# \{0,1\}//' ;;
  esac
}

main "$@"
