#!/usr/bin/env bash
#
# kolibri.sh - Script de gestion para Kolibri
# Uso: ./kolibri.sh [comando]
# Sin argumentos: abre menu interactivo
#

set -uo pipefail

# ── Colores ──────────────────────────────────────────────────────────────────
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
CYAN='\033[0;36m'
MAGENTA='\033[0;35m'
BOLD='\033[1m'
DIM='\033[2m'
NC='\033[0m'

# ── Variables ────────────────────────────────────────────────────────────────
PROJECT_DIR="$(cd "$(dirname "$0")" && pwd)"
TAURI_DIR="$PROJECT_DIR/src-tauri"
BUILD_DIR="$PROJECT_DIR/build"
BINARY_NAME="kolibri"
APP_IDENTIFIER="com.kolibri.app"
VERSION=$(grep '"version"' "$PROJECT_DIR/package.json" | head -1 | sed 's/.*: *"\(.*\)".*/\1/')

_detect_cargo_target() {
    if [[ -n "${CARGO_TARGET_DIR:-}" ]]; then
        echo "$CARGO_TARGET_DIR"
    elif grep -q 'target-dir' ~/.cargo/config.toml 2>/dev/null; then
        grep 'target-dir' ~/.cargo/config.toml | head -1 | sed 's/.*= *"\(.*\)".*/\1/' | sed "s|~|$HOME|"
    else
        echo "$TAURI_DIR/target"
    fi
}
CARGO_TARGET="$(_detect_cargo_target)"
BUNDLE_DIR="$CARGO_TARGET/release/bundle"

find_binary() {
    local name="${1:-$BINARY_NAME}"
    for dir in "$CARGO_TARGET/release" "$TAURI_DIR/target/release"; do
        if [[ -f "$dir/$name" ]]; then
            echo "$dir/$name"
            return 0
        fi
    done
    return 1
}

# ── Funciones auxiliares ─────────────────────────────────────────────────────
info()    { echo -e "${BLUE}[INFO]${NC} $*"; }
success() { echo -e "${GREEN}[OK]${NC} $*"; }
warn()    { echo -e "${YELLOW}[WARN]${NC} $*"; }
error()   { echo -e "${RED}[ERROR]${NC} $*" >&2; }
header()  { echo -e "\n${BOLD}${CYAN}=== $* ===${NC}\n"; }

elapsed() {
    local start=$1
    local end=$(date +%s)
    local diff=$((end - start))
    echo "$((diff / 60))m $((diff % 60))s"
}

pause_after() {
    echo ""
    echo -e "${DIM}Presiona ENTER para volver al menu...${NC}"
    read -r
}

# ── Verificacion de dependencias ─────────────────────────────────────────────
check_deps() {
    header "Verificando dependencias"
    local missing=0

    for cmd in node pnpm cargo rustc; do
        if command -v "$cmd" &>/dev/null; then
            success "$cmd -> $(command $cmd --version 2>/dev/null | head -1)"
        else
            error "$cmd no encontrado"
            missing=1
        fi
    done

    if pnpm tauri --version &>/dev/null; then
        success "tauri-cli -> $(pnpm tauri --version 2>/dev/null)"
    else
        error "tauri-cli no encontrado (pnpm i -D @tauri-apps/cli)"
        missing=1
    fi

    if [[ $missing -eq 1 ]]; then
        error "Faltan dependencias obligatorias"
        return 1
    fi
    success "Todas las dependencias disponibles"
}

# ── Instalar dependencias ────────────────────────────────────────────────────
cmd_install() {
    header "Instalando dependencias"
    cd "$PROJECT_DIR"
    pnpm install
    success "Dependencias pnpm instaladas"
}

# ── Desarrollo ───────────────────────────────────────────────────────────────
cmd_dev() {
    header "Modo desarrollo (Tauri)"
    check_deps || return
    cd "$PROJECT_DIR"
    info "Iniciando Tauri + Vite hot reload..."
    pnpm tauri dev || true
}

cmd_dev_web() {
    header "Frontend dev (solo navegador)"
    cd "$PROJECT_DIR"
    info "Iniciando Vite dev server... (Ctrl+C para detener)"
    pnpm dev || true
}

# ── Check ────────────────────────────────────────────────────────────────────
cmd_check() {
    header "Verificacion rapida"
    local start=$(date +%s)
    local errors=0

    info "Svelte check..."
    cd "$PROJECT_DIR"
    if pnpm check; then
        success "Svelte/TS OK"
    else
        error "Svelte/TS tiene errores"
        errors=1
    fi

    info "Cargo check..."
    cd "$TAURI_DIR"
    if cargo check; then
        success "Rust OK"
    else
        error "Rust tiene errores"
        errors=1
    fi

    if [[ $errors -eq 0 ]]; then
        success "Todo OK en $(elapsed $start)"
    else
        error "Hay errores ($(elapsed $start))"
    fi
}

# ── Build ────────────────────────────────────────────────────────────────────
cmd_build() {
    header "Build Kolibri v$VERSION"
    check_deps || return
    local start=$(date +%s)
    cd "$PROJECT_DIR"

    info "Compilando release completo..."
    pnpm tauri build

    success "Build completo en $(elapsed $start)"
    collect_artifacts
}

cmd_build_debug() {
    header "Build debug"
    cd "$PROJECT_DIR"
    local start=$(date +%s)

    info "Compilando en modo debug..."
    pnpm tauri build --debug

    success "Build debug en $(elapsed $start)"
}

cmd_build_frontend() {
    header "Build frontend"
    local start=$(date +%s)
    cd "$PROJECT_DIR"

    info "Vite build..."
    pnpm build

    success "Frontend compilado en $(elapsed $start)"
}

# ── Ejecutar binario ─────────────────────────────────────────────────────────
cmd_run() {
    local bin="$(find_binary || echo '')"
    if [[ ! -f "$bin" ]]; then
        error "Binario no encontrado. Ejecuta 'build' primero."
        return 1
    fi
    header "Ejecutando Kolibri v$VERSION"
    "$bin" "$@" || true
}

# ── Recopilar artefactos ─────────────────────────────────────────────────────
collect_artifacts() {
    local out="$PROJECT_DIR/out/$(date '+%Y-%m-%d_%H-%M')"
    mkdir -p "$out"

    local bin="$(find_binary || echo '')"
    if [[ -f "$bin" ]]; then
        cp "$bin" "$out/"
        success "Binario en: $out"
    fi

    if [[ -d "$BUNDLE_DIR" ]]; then
        local patterns=("Kolibri*" "kolibri*")
        for type in deb rpm appimage msi nsis dmg; do
            [[ -d "$BUNDLE_DIR/$type" ]] || continue
            for pat in "${patterns[@]}"; do
                for item in "$BUNDLE_DIR/$type"/$pat; do
                    [[ -e "$item" ]] && cp -r "$item" "$out/" 2>/dev/null || true
                done
            done
        done
    fi

    if [[ -n "$(ls -A "$out" 2>/dev/null)" ]]; then
        ls -lh "$out" | tail -n +2
    else
        rmdir "$out" 2>/dev/null
        warn "No se encontraron artefactos"
    fi
}

# ── Datos (services.json + sessions/) ────────────────────────────────────────
cmd_data_path() {
    header "Ubicacion de datos"
    echo -e "${BOLD}Linux:${NC}   ~/.local/share/$APP_IDENTIFIER/"
    echo -e "${BOLD}Windows:${NC} %APPDATA%\\$APP_IDENTIFIER\\"
    echo -e "${BOLD}macOS:${NC}   ~/Library/Application Support/$APP_IDENTIFIER/"
    echo ""
    local data_dir="$HOME/.local/share/$APP_IDENTIFIER"
    if [[ -d "$data_dir" ]]; then
        success "Existe: $data_dir ($(du -sh "$data_dir" 2>/dev/null | cut -f1))"
        local cfg="$data_dir/services.json"
        if [[ -f "$cfg" ]]; then
            local n_svcs
            n_svcs=$(python3 -c "import json; print(len(json.load(open('$cfg')).get('services',[])))" 2>/dev/null || echo "?")
            echo -e "${BOLD}services.json:${NC} $n_svcs servicio(s)"
        fi
        if [[ -d "$data_dir/sessions" ]]; then
            local n_sess
            n_sess=$(ls -1d "$data_dir/sessions"/*/ 2>/dev/null | wc -l)
            echo -e "${BOLD}sessions/:${NC} $n_sess carpeta(s) ($(du -sh "$data_dir/sessions" 2>/dev/null | cut -f1))"
        fi
    else
        warn "Directorio no existe aun (corre la app primero)"
    fi
}

cmd_data_reset() {
    header "Reset config Kolibri"
    local data_dir="$HOME/.local/share/$APP_IDENTIFIER"
    local cfg="$data_dir/services.json"
    if [[ ! -f "$cfg" ]]; then
        info "No se encontro services.json"
        return
    fi
    warn "Encontrada config en: $cfg"
    warn "Esto borra services.json (no toca sessions/ con cookies)"
    read -rp "¿Eliminar? (s/N) " ans
    if [[ "$ans" =~ ^[sS]$ ]]; then
        rm -f "$cfg"
        success "services.json eliminado"
    fi
}

cmd_data_purge() {
    header "Purgar TODA la data Kolibri"
    local data_dir="$HOME/.local/share/$APP_IDENTIFIER"
    if [[ ! -d "$data_dir" ]]; then
        info "Nada que purgar"
        return
    fi
    warn "Esto borra config + sessions (cookies de WhatsApp, Gmail, etc)"
    warn "Tendras que escanear QR / loguearte de nuevo en cada servicio"
    read -rp "¿Eliminar TODO? (s/N) " ans
    if [[ "$ans" =~ ^[sS]$ ]]; then
        rm -rf "$data_dir"
        success "Datos purgados"
    fi
}

# ── Limpiar ──────────────────────────────────────────────────────────────────
cmd_clean() {
    header "Limpieza"
    info "Limpiando build/ + .svelte-kit + cargo clean..."
    rm -rf "$BUILD_DIR" "$PROJECT_DIR/.svelte-kit"
    cd "$TAURI_DIR" && cargo clean
    success "Limpio"
}

# ── Info ─────────────────────────────────────────────────────────────────────
cmd_info() {
    header "Kolibri v$VERSION"
    echo -e "${BOLD}Directorio:${NC} $PROJECT_DIR"
    echo -e "${BOLD}Node:${NC}       $(node --version 2>/dev/null || echo 'N/A')"
    echo -e "${BOLD}pnpm:${NC}       $(pnpm --version 2>/dev/null || echo 'N/A')"
    echo -e "${BOLD}Rust:${NC}       $(rustc --version 2>/dev/null || echo 'N/A')"
    echo -e "${BOLD}Tauri:${NC}      $(pnpm tauri --version 2>/dev/null || echo 'N/A')"
    echo -e "${BOLD}Branch:${NC}     $(git -C "$PROJECT_DIR" branch --show-current 2>/dev/null || echo 'N/A')"
    echo -e "${BOLD}Commit:${NC}     $(git -C "$PROJECT_DIR" log --oneline -1 2>/dev/null || echo 'N/A')"
    local bin="$(find_binary || echo '')"
    if [[ -f "$bin" ]]; then
        echo -e "${BOLD}Binario:${NC}    $bin ($(du -h "$bin" | cut -f1))"
    fi
}

# ── Release ──────────────────────────────────────────────────────────────────
cmd_release() {
    header "Nuevo release"
    cd "$PROJECT_DIR"

    if ! command -v gh &>/dev/null; then
        error "gh (GitHub CLI) no instalado"
        return 1
    fi

    if [[ -n "$(git status --porcelain)" ]]; then
        error "Hay cambios sin commitear. Commitea o stashea antes."
        git status --short
        return 1
    fi

    local branch=$(git branch --show-current)
    if [[ "$branch" != "main" ]]; then
        warn "No estas en main (actual: $branch)"
        read -rp "¿Continuar de todas formas? (s/N) " ans
        [[ "$ans" =~ ^[sS]$ ]] || return 1
    fi

    local current="$VERSION"
    local IFS='.'
    read -r major minor patch <<< "$current"
    unset IFS

    if [[ -z "$major" || -z "$minor" || -z "$patch" ]]; then
        error "Versión actual invalida: $current"
        return 1
    fi

    local next_major="$((major + 1)).0.0"
    local next_minor="$major.$((minor + 1)).0"
    local next_patch="$major.$minor.$((patch + 1))"

    echo -e "${BOLD}Version actual:${NC} ${CYAN}v$current${NC}"
    echo ""
    echo -e "  ${GREEN}1${NC}) patch  → v${next_patch}  ${DIM}(fixes, cambios menores)${NC}"
    echo -e "  ${YELLOW}2${NC}) minor  → v${next_minor}  ${DIM}(features nuevas compatibles)${NC}"
    echo -e "  ${RED}3${NC}) major  → v${next_major}  ${DIM}(cambios breaking)${NC}"
    echo -e "  ${BLUE}4${NC}) custom ${DIM}(escribir version manual)${NC}"
    echo -e "  ${DIM}0) cancelar${NC}"
    echo ""
    read -rp "Opcion: " opt

    local new_version=""
    case "$opt" in
        1) new_version="$next_patch" ;;
        2) new_version="$next_minor" ;;
        3) new_version="$next_major" ;;
        4)
            read -rp "Nueva version (X.Y.Z): " new_version
            if [[ ! "$new_version" =~ ^[0-9]+\.[0-9]+\.[0-9]+$ ]]; then
                error "Formato invalido. Debe ser X.Y.Z"
                return 1
            fi
            ;;
        0|"") info "Cancelado"; return 0 ;;
        *) error "Opcion no valida"; return 1 ;;
    esac

    if git rev-parse "v$new_version" &>/dev/null; then
        error "Tag v$new_version ya existe"
        return 1
    fi

    local last_tag=$(git describe --tags --abbrev=0 2>/dev/null || echo "")
    echo ""
    echo -e "${BOLD}Commits desde ${last_tag:-inicio}:${NC}"
    if [[ -n "$last_tag" ]]; then
        git log "${last_tag}..HEAD" --oneline | head -30
    else
        git log --oneline | head -30
    fi
    echo ""

    echo -e "${BOLD}Resumen:${NC}"
    echo -e "  v${CYAN}$current${NC} → v${GREEN}$new_version${NC}"
    echo -e "  - Actualiza package.json, tauri.conf.json, Cargo.toml, Cargo.lock"
    echo -e "  - Crea commit chore(release): v$new_version"
    echo -e "  - Crea tag v$new_version"
    echo -e "  - Push a origin/main + tag"
    echo ""
    read -rp "¿Confirmar? (s/N) " confirm
    [[ "$confirm" =~ ^[sS]$ ]] || { info "Cancelado"; return 0; }

    info "Actualizando archivos de version..."
    sed -i "s/\"version\": \"$current\"/\"version\": \"$new_version\"/" "$PROJECT_DIR/package.json"
    sed -i "s/\"version\": \"$current\"/\"version\": \"$new_version\"/" "$TAURI_DIR/tauri.conf.json"
    sed -i "s/^version = \"$current\"/version = \"$new_version\"/" "$TAURI_DIR/Cargo.toml"
    if [[ -f "$TAURI_DIR/Cargo.lock" ]]; then
        python3 -c "
import re
p = '$TAURI_DIR/Cargo.lock'
with open(p) as f: s = f.read()
s = re.sub(r'(name = \"$BINARY_NAME\"\nversion = \")$current(\")', r'\g<1>$new_version\g<2>', s)
with open(p, 'w') as f: f.write(s)
" 2>/dev/null || {
            warn "python3 no disponible, usando sed para Cargo.lock (menos seguro)"
            sed -i "/^name = \"$BINARY_NAME\"$/,/^version = / s/^version = \"$current\"/version = \"$new_version\"/" "$TAURI_DIR/Cargo.lock"
        }
    fi

    success "Versiones actualizadas a $new_version"

    info "Creando commit..."
    git add package.json "$TAURI_DIR/tauri.conf.json" "$TAURI_DIR/Cargo.toml"
    [[ -f "$TAURI_DIR/Cargo.lock" ]] && git add "$TAURI_DIR/Cargo.lock"
    git commit -m "chore(release): v$new_version"

    info "Creando tag v$new_version..."
    git tag -a "v$new_version" -m "Kolibri v$new_version"

    info "Push a origin..."
    git push origin "$branch"
    git push origin "v$new_version"

    success "Release v$new_version publicado"
}

# ══════════════════════════════════════════════════════════════════════════════
# ── MENU INTERACTIVO ─────────────────────────────────────────────────────────
# ══════════════════════════════════════════════════════════════════════════════

show_banner() {
    clear
    echo -e "${BOLD}${MAGENTA}"
    echo " _  __    _ _ _          _ "
    echo "| |/ /___| (_) |__  _ __(_)"
    echo "| ' // _ \ | | '_ \| '__| |"
    echo "| . \ (_) | | | |_) | |  | |"
    echo "|_|\_\___/|_|_|_.__/|_|  |_|"
    echo -e "${NC}"
    echo -e "${DIM}  Cliente liviano multi-servicio · v$VERSION"
    echo -e "  $(git -C "$PROJECT_DIR" branch --show-current 2>/dev/null || echo '-') · $(git -C "$PROJECT_DIR" log --oneline -1 2>/dev/null | cut -c1-50 || echo '-')${NC}"
    echo ""
}

show_menu() {
    echo -e "${BOLD} DESARROLLO${NC}"
    echo -e "  ${GREEN}1${NC})  Dev Tauri          ${DIM}Tauri + Vite hot reload${NC}"
    echo -e "  ${GREEN}2${NC})  Dev Web            ${DIM}Solo frontend en navegador${NC}"
    echo -e "  ${GREEN}3${NC})  Check              ${DIM}svelte-check + cargo check${NC}"
    echo ""
    echo -e "${BOLD} BUILD${NC}"
    echo -e "  ${YELLOW}4${NC})  Build release      ${DIM}App completa + paquetes${NC}"
    echo -e "  ${YELLOW}5${NC})  Build debug        ${DIM}Sin optimizaciones${NC}"
    echo -e "  ${YELLOW}6${NC})  Build frontend     ${DIM}Solo vite build${NC}"
    echo ""
    echo -e "${BOLD} GESTION${NC}"
    echo -e "  ${BLUE}7${NC})  Ejecutar app       ${DIM}Lanzar binario release${NC}"
    echo -e "  ${BLUE}8${NC})  Instalar deps      ${DIM}pnpm install${NC}"
    echo -e "  ${BLUE}9${NC})  Info proyecto      ${DIM}Versiones y estado${NC}"
    echo ""
    echo -e "${BOLD} DATOS${NC}"
    echo -e "  ${CYAN}10${NC}) Ver path datos     ${DIM}services.json + sessions/${NC}"
    echo -e "  ${RED}11${NC}) Reset config       ${DIM}Borrar services.json${NC}"
    echo -e "  ${RED}12${NC}) Purgar TODO        ${DIM}config + sessions (logout total)${NC}"
    echo -e "  ${RED}13${NC}) Limpiar build      ${DIM}build/ + .svelte-kit + cargo clean${NC}"
    echo ""
    echo -e "${BOLD} RELEASE${NC}"
    echo -e "  ${MAGENTA}14${NC}) Nuevo release     ${DIM}Bump version + tag + push${NC}"
    echo ""
    echo -e "  ${BOLD}0${NC})  Salir"
    echo ""
}

menu_loop() {
    while true; do
        show_banner
        show_menu

        echo -ne "${BOLD}  Opcion: ${NC}"
        read -r choice

        case "${choice// /}" in
            1)  cmd_dev;            pause_after ;;
            2)  cmd_dev_web;        pause_after ;;
            3)  cmd_check;          pause_after ;;
            4)  cmd_build;          pause_after ;;
            5)  cmd_build_debug;    pause_after ;;
            6)  cmd_build_frontend; pause_after ;;
            7)  cmd_run;            pause_after ;;
            8)  cmd_install;        pause_after ;;
            9)  cmd_info;           pause_after ;;
            10) cmd_data_path;      pause_after ;;
            11) cmd_data_reset;     pause_after ;;
            12) cmd_data_purge;     pause_after ;;
            13) cmd_clean;          pause_after ;;
            14) cmd_release;        pause_after ;;
            0|q|salir) echo -e "\n${GREEN}Hasta luego${NC}"; exit 0 ;;
            "") ;;
            *)  error "Opcion no valida: $choice"; sleep 1 ;;
        esac
    done
}

# ── Ayuda CLI ────────────────────────────────────────────────────────────────
cmd_help() {
    echo -e "${BOLD}${MAGENTA}Kolibri v$VERSION${NC}"
    echo -e "${DIM}Cliente liviano multi-servicio${NC}"
    echo ""
    echo -e "${BOLD}Uso:${NC} ./kolibri.sh [comando]"
    echo -e "     ./kolibri.sh          ${DIM}(menu interactivo)${NC}"
    echo ""
    echo "  dev            Tauri + Vite hot reload"
    echo "  dev:web        Solo frontend en navegador"
    echo "  check          svelte-check + cargo check"
    echo "  build          Build release completo"
    echo "  build:debug    Build debug"
    echo "  build:frontend Solo frontend"
    echo "  run            Ejecutar binario release"
    echo "  install        pnpm install"
    echo "  info           Info del proyecto"
    echo "  data:path      Ubicacion de los datos"
    echo "  data:reset     Borrar services.json"
    echo "  data:purge     Borrar TODO (config + cookies)"
    echo "  clean          Limpiar build/ + cargo clean"
    echo "  release        Nuevo release (bump + tag + push)"
    echo "  help           Esta ayuda"
}

# ── Router ───────────────────────────────────────────────────────────────────
main() {
    cd "$PROJECT_DIR"

    if [[ $# -eq 0 ]]; then
        menu_loop
        exit 0
    fi

    case "$1" in
        dev)            cmd_dev ;;
        dev:web)        cmd_dev_web ;;
        check)          cmd_check ;;
        build)          cmd_build ;;
        build:debug)    cmd_build_debug ;;
        build:frontend) cmd_build_frontend ;;
        run)            cmd_run ;;
        install)        cmd_install ;;
        info)           cmd_info ;;
        data:path)      cmd_data_path ;;
        data:reset)     cmd_data_reset ;;
        data:purge)     cmd_data_purge ;;
        clean)          cmd_clean ;;
        release)        cmd_release ;;
        help|--help|-h) cmd_help ;;
        *)              error "Comando desconocido: $1"; cmd_help; exit 1 ;;
    esac
}

main "$@"
