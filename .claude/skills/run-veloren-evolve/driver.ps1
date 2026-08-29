<#
  Pilote du client Veloren (voxygen) pour agent.

  Toutes les coordonnees sont celles de l'AIRE CLIENT, donc identiques a
  celles lues sur une image produite par -Action shot. C'est deliberé :
  capturer le rectangle de fenetre au lieu de l'aire client decale tout de
  la hauteur de la barre de titre, et les clics ratent leur cible.

  pwsh -File driver.ps1 -Action <launch|fit|shot|click|key|text|look|zoom|walk|state|stop>
#>
param(
  [Parameter(Mandatory=$true)]
  [ValidateSet('launch','fit','shot','click','drag','press','key','text','look','zoom','walk','state','stop')]
  [string]$Action,
  [ValidateSet('left','right','middle')]
  [string]$Button = 'left',
  [int]$X = -1, [int]$Y = -1,
  [int]$X2 = -1, [int]$Y2 = -1,
  [int]$Dx = 0, [int]$Dy = 0,
  [int]$Ticks = 0,
  [double]$Seconds = 2,
  [string]$Value = '',
  [string]$Out = '',
  [string]$OutDir = '',
  [int]$Width = 1500, [int]$Height = 930
)

$ErrorActionPreference = 'Stop'
if (-not $OutDir) { $OutDir = Join-Path $env:TEMP 'veloren-run' }
New-Item -ItemType Directory -Force $OutDir | Out-Null

# racine du fork : trois niveaux au-dessus de .claude/skills/run-veloren-evolve/
$Repo = (Resolve-Path (Join-Path $PSScriptRoot '..\..\..')).Path
$Exe  = Join-Path $Repo 'target\no_overflow\veloren-voxygen.exe'

Add-Type -AssemblyName System.Drawing
Add-Type @"
using System; using System.Runtime.InteropServices;
public class Vx {
  [DllImport("user32.dll")] public static extern bool SetProcessDPIAware();
  [DllImport("user32.dll")] public static extern IntPtr GetForegroundWindow();
  [DllImport("user32.dll")] public static extern uint GetWindowThreadProcessId(IntPtr h, IntPtr p);
  [DllImport("kernel32.dll")] public static extern uint GetCurrentThreadId();
  [DllImport("user32.dll")] public static extern bool AttachThreadInput(uint a, uint b, bool f);
  [DllImport("user32.dll")] public static extern bool SetForegroundWindow(IntPtr h);
  [DllImport("user32.dll")] public static extern IntPtr SetFocus(IntPtr h);
  [DllImport("user32.dll")] public static extern bool BringWindowToTop(IntPtr h);
  [DllImport("user32.dll")] public static extern bool ShowWindow(IntPtr h, int n);
  [DllImport("user32.dll")] public static extern bool SetWindowPos(IntPtr h, IntPtr a, int x, int y, int w, int ht, uint f);
  [DllImport("user32.dll")] public static extern bool GetClientRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool GetWindowRect(IntPtr h, out RECT r);
  [DllImport("user32.dll")] public static extern bool ClientToScreen(IntPtr h, ref PT p);
  [DllImport("user32.dll")] public static extern bool SetCursorPos(int x, int y);
  [DllImport("user32.dll")] public static extern void mouse_event(uint f, int x, int y, uint d, IntPtr e);
  [DllImport("user32.dll")] public static extern void keybd_event(byte vk, byte sc, uint f, IntPtr e);
  [DllImport("user32.dll")] public static extern short VkKeyScan(char c);
  [DllImport("user32.dll")] public static extern IntPtr PostMessage(IntPtr h, uint m, IntPtr w, IntPtr l);
  [DllImport("user32.dll")] public static extern int GetSystemMetrics(int i);
  [StructLayout(LayoutKind.Sequential)] public struct RECT { public int L,T,R,B; }
  [StructLayout(LayoutKind.Sequential)] public struct PT { public int X,Y; }
}
"@
# OBLIGATOIRE : l'ecran peut etre a 125%, winit travaille en pixels physiques.
# Sans cet appel, PowerShell voit un bureau virtualise et les clics tombent
# environ 25% trop loin.
[void][Vx]::SetProcessDPIAware()

function Get-Game {
  $p = Get-Process -Name veloren-voxygen -ErrorAction SilentlyContinue | Select-Object -First 1
  if (-not $p) { throw "veloren-voxygen ne tourne pas (lancer -Action launch)" }
  if ($p.MainWindowHandle -eq [IntPtr]::Zero) { throw "fenetre pas encore creee" }
  return $p
}

function Get-Client($h) {
  $r = New-Object Vx+RECT; [void][Vx]::GetClientRect($h, [ref]$r)
  $o = New-Object Vx+PT;   [void][Vx]::ClientToScreen($h, [ref]$o)
  return [PSCustomObject]@{ W = $r.R; H = $r.B; SX = $o.X; SY = $o.Y }
}

# Sans ce bloc, les entrees injectees n'atteignent pas la fenetre de facon
# fiable, meme quand GetForegroundWindow la designe deja.
function Grab-Focus($h) {
  $script:tFg = [Vx]::GetWindowThreadProcessId([Vx]::GetForegroundWindow(), [IntPtr]::Zero)
  $script:tMe = [Vx]::GetCurrentThreadId()
  $script:tG  = [Vx]::GetWindowThreadProcessId($h, [IntPtr]::Zero)
  [void][Vx]::AttachThreadInput($script:tMe, $script:tFg, $true)
  [void][Vx]::AttachThreadInput($script:tMe, $script:tG,  $true)
  [void][Vx]::ShowWindow($h, 9)
  [void][Vx]::BringWindowToTop($h)
  [void][Vx]::SetForegroundWindow($h)
  [void][Vx]::SetFocus($h)
  Start-Sleep -Milliseconds 450
}

function Release-Focus {
  [void][Vx]::AttachThreadInput($script:tMe, $script:tG,  $false)
  [void][Vx]::AttachThreadInput($script:tMe, $script:tFg, $false)
}

# Les touches nommees sont fixes ; tout le reste passe par la DISPOSITION du
# clavier. C'est indispensable : Veloren lie ses entrees a des caracteres
# logiques (`Key::Character("2")`), et sur un clavier AZERTY la rangee de
# chiffres demande Maj. Envoyer le code virtuel 0x32 y produit « e accent »,
# que le jeu ignore en silence — la touche semble alors perdue.
function Get-Vk([string]$name) {
  $m = @{ enter = 0x0D; esc = 0x1B; space = 0x20; tab = 0x09; back = 0x08; del = 0x2E;
          f1 = 0x70; f4 = 0x73; f11 = 0x7A; up = 0x26; down = 0x28; left = 0x25; right = 0x27 }
  $v = $m[$name.ToLower()]
  if ($v) { return @{ Vk = [byte]$v; Shift = $false } }
  if ($name.Length -ne 1) { throw "touche inconnue : $name" }
  $sc = [Vx]::VkKeyScan([char]$name)
  if ($sc -eq -1) { throw "caractere absent de la disposition : $name" }
  return @{ Vk = [byte]($sc -band 0xFF); Shift = ((($sc -shr 8) -band 1) -eq 1) }
}

function Send-Key($k) {
  if ($k.Shift) { [Vx]::keybd_event(0x10, 0, 0, [IntPtr]::Zero) }
  [Vx]::keybd_event($k.Vk, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 70
  [Vx]::keybd_event($k.Vk, 0, 2, [IntPtr]::Zero)
  if ($k.Shift) { [Vx]::keybd_event(0x10, 0, 2, [IntPtr]::Zero) }
}

# Le swapchain Vulkan de voxygen presente en contournant le compositeur : GDI
# relit alors une surface figee, et deux captures a une minute d'intervalle
# sortent identiques au bit pres. Reappliquer SetWindowPos avec la geometrie
# courante force DWM a recomposer, ce qui rafraichit ce que GDI peut lire.
# Sans ce reveil, -Action shot ment.
function Wake-Compositor($h) {
  $r = New-Object Vx+RECT; [void][Vx]::GetWindowRect($h, [ref]$r)
  [void][Vx]::SetWindowPos($h, [IntPtr]0, $r.L, $r.T, $r.R - $r.L, $r.B - $r.T, 0x0040)
  Start-Sleep -Milliseconds 700
}

# Capture l'AIRE CLIENT seule : les coordonnees de l'image sont exactement
# celles attendues par -Action click.
#
# On prend TOUT l'ecran puis on recadre, au lieu de copier directement le
# rectangle de l'aire client. Ce n'est pas un detour gratuit : copier le seul
# rectangle client ressort parfois entierement blanc, la ou la meme copie en
# plein ecran rend l'image juste. Le blanc n'est pas une erreur, c'est une
# image plausible — donc un alibi. Le plein ecran coute quelques millisecondes
# et ne ment pas.
function Save-Shot($h, $path) {
  Wake-Compositor $h
  $c = Get-Client $h
  if ($c.W -le 0 -or $c.H -le 0) { throw "aire client vide" }
  $ew = [Vx]::GetSystemMetrics(0); $eh = [Vx]::GetSystemMetrics(1)
  $plein = New-Object System.Drawing.Bitmap $ew, $eh
  $g = [System.Drawing.Graphics]::FromImage($plein)
  $g.CopyFromScreen(0, 0, 0, 0, $plein.Size)
  $g.Dispose()
  $rect = New-Object System.Drawing.Rectangle $c.SX, $c.SY, $c.W, $c.H
  $bmp = $plein.Clone($rect, $plein.PixelFormat)
  $bmp.Save($path, [System.Drawing.Imaging.ImageFormat]::Png)
  $bmp.Dispose(); $plein.Dispose()
  return "$path ($($c.W)x$($c.H))"
}

switch ($Action) {

  'launch' {
    if (-not (Test-Path $Exe)) {
      throw "binaire absent : $Exe`n  cargo build --profile no_overflow --bin veloren-voxygen"
    }
    $st = Join-Path $Repo 'userdata\voxygen\settings.ron'
    if (Test-Path $st) {
      # Le plein ecran sans bordure fait passer Windows en independent flip :
      # la capture GDI ne renvoie plus qu'une image figee. On force le fenetre.
      $txt = Get-Content $st -Raw
      if ($txt -match '(?s)fullscreen:\s*\(\s*\r?\n\s*enabled:\s*true') {
        $txt = $txt -replace '(?s)(fullscreen:\s*\(\s*\r?\n\s*enabled:\s*)true', '${1}false'
        Set-Content $st $txt -NoNewline
        "settings.ron : plein ecran desactive"
      }
      $txt = Get-Content $st -Raw
      if ($txt -match 'show_disclaimer:\s*true') {
        $txt -replace 'show_disclaimer:\s*true', 'show_disclaimer: false' | Set-Content $st -NoNewline
        "settings.ron : avertissement de demarrage desactive"
      }
    }
    $env:VELOREN_ASSETS = Join-Path $Repo 'assets'
    $env:VOXYGEN_SCREENSHOT = $OutDir
    $p = Start-Process -FilePath $Exe -WorkingDirectory $Repo `
      -RedirectStandardOutput (Join-Path $OutDir 'game.out') `
      -RedirectStandardError  (Join-Path $OutDir 'game.err') -PassThru
    "PID $($p.Id) ; journal $OutDir\game.out"
    # Le demarrage compile ~45 pipelines de shaders : compter 40 a 60 s.
    # On attend la ligne egui_wgpu, qui suit la derniere compilation.
    $ready = $false
    $log = Join-Path $OutDir 'game.out'
    foreach ($i in 1..90) {
      Start-Sleep -Seconds 2
      if (-not (Get-Process -Id $p.Id -ErrorAction SilentlyContinue)) {
        throw "process mort au demarrage ; voir $OutDir\game.err"
      }
      if ((Test-Path $log) -and (Select-String -Path $log -Pattern 'egui_wgpu' -Quiet)) { $ready = $true; break }
    }
    if (-not $ready) { throw "fenetre pas prete apres 180 s ; voir $log" }
    Start-Sleep -Seconds 3
    $h = (Get-Game).MainWindowHandle
    [void][Vx]::SetWindowPos($h, [IntPtr]0, 0, 0, $Width, $Height, 0x0040)
    Start-Sleep -Seconds 2
    $c = Get-Client $h
    "pret. client $($c.W)x$($c.H) a l'ecran $($c.SX),$($c.SY)"
  }

  'fit' {
    # La fenetre ne doit jamais deborder sous la barre des taches : les boutons
    # du bas deviennent alors incliquables, les clics allant a la barre.
    $h = (Get-Game).MainWindowHandle
    [void][Vx]::SetWindowPos($h, [IntPtr]0, 0, 0, $Width, $Height, 0x0040)
    Start-Sleep -Seconds 2
    $c = Get-Client $h
    "client $($c.W)x$($c.H) a $($c.SX),$($c.SY) ; bas a $($c.SY + $c.H) ; hauteur ecran $([Vx]::GetSystemMetrics(1))"
  }

  'shot' {
    $h = (Get-Game).MainWindowHandle
    if (-not $Out) { $Out = Join-Path $OutDir ("shot-" + (Get-Date -Format 'HHmmss') + ".png") }
    Save-Shot $h $Out
  }

  'click' {
    if ($X -lt 0 -or $Y -lt 0) { throw "-X et -Y requis (coordonnees lues sur une image shot)" }
    $h = (Get-Game).MainWindowHandle
    Grab-Focus $h
    $c = Get-Client $h
    # Approche progressive : un saut sec du curseur ne produit pas toujours le
    # WM_MOUSEMOVE dont l'interface a besoin pour savoir ce qui est survole.
    foreach ($k in 1..4) {
      [void][Vx]::SetCursorPos($c.SX + $X - 60 + 15 * $k, $c.SY + $Y)
      Start-Sleep -Milliseconds 80
    }
    [void][Vx]::SetCursorPos($c.SX + $X, $c.SY + $Y)
    Start-Sleep -Milliseconds 300
    [Vx]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 90
    [Vx]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)
    Release-Focus
    "clic $X,$Y"
  }

  'drag' {
    # Glisser-deposer d'une case d'interface a une autre : c'est le seul geste
    # qui garnit la barre d'objets, le clic simple n'ouvrant qu'un menu
    # d'actions depuis la refonte du sac.
    if ($X -lt 0 -or $Y -lt 0 -or $X2 -lt 0 -or $Y2 -lt 0) {
      throw "-X -Y -X2 -Y2 requis (coordonnees lues sur une image shot)"
    }
    $h = (Get-Game).MainWindowHandle
    Grab-Focus $h
    $c = Get-Client $h
    [void][Vx]::SetCursorPos($c.SX + $X, $c.SY + $Y)
    Start-Sleep -Milliseconds 250
    [Vx]::mouse_event(0x0002, 0, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 200
    # Le trajet se fait en plusieurs pas : un saut sec ne produit pas toujours
    # le WM_MOUSEMOVE dont l'interface a besoin pour suivre l'objet saisi.
    foreach ($k in 1..8) {
      [void][Vx]::SetCursorPos($c.SX + $X + [int](($X2 - $X) * $k / 8),
                               $c.SY + $Y + [int](($Y2 - $Y) * $k / 8))
      Start-Sleep -Milliseconds 60
    }
    Start-Sleep -Milliseconds 250
    [Vx]::mouse_event(0x0004, 0, 0, 0, [IntPtr]::Zero)
    Release-Focus
    "glisse $X,$Y vers $X2,$Y2"
  }

  'press' {
    # Clic a la position courante du curseur, sans le deplacer. C'est ce qu'il
    # faut en jeu, ou le curseur est capture et ou la cible est le reticule :
    # deplacer le curseur ferait pivoter la camera.
    $h = (Get-Game).MainWindowHandle; Grab-Focus $h
    $down = @{ left = 0x0002; right = 0x0008; middle = 0x0020 }[$Button]
    $up   = @{ left = 0x0004; right = 0x0010; middle = 0x0040 }[$Button]
    [Vx]::mouse_event([uint32]$down, 0, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Milliseconds 110
    [Vx]::mouse_event([uint32]$up, 0, 0, 0, [IntPtr]::Zero)
    Release-Focus
    "clic $Button au reticule"
  }

  'key' {
    $h = (Get-Game).MainWindowHandle; Grab-Focus $h
    Send-Key (Get-Vk $Value)
    Release-Focus
    "touche $Value"
  }

  'text' {
    $h = (Get-Game).MainWindowHandle; Grab-Focus $h
    foreach ($ch in $Value.ToCharArray()) {
      $sc = [Vx]::VkKeyScan($ch)
      $v = [byte]($sc -band 0xFF)
      $shift = (($sc -shr 8) -band 1) -eq 1
      if ($shift) { [Vx]::keybd_event(0x10, 0, 0, [IntPtr]::Zero) }
      [Vx]::keybd_event($v, 0, 0, [IntPtr]::Zero); Start-Sleep -Milliseconds 25
      [Vx]::keybd_event($v, 0, 2, [IntPtr]::Zero)
      if ($shift) { [Vx]::keybd_event(0x10, 0, 2, [IntPtr]::Zero) }
      Start-Sleep -Milliseconds 35
    }
    Release-Focus
    "texte '$Value'"
  }

  'look' {
    # En jeu le curseur est capture : la camera se pilote en mouvement RELATIF.
    $h = (Get-Game).MainWindowHandle; Grab-Focus $h
    $n = 20
    foreach ($i in 1..$n) {
      [Vx]::mouse_event(0x0001, [int]($Dx / $n), [int]($Dy / $n), 0, [IntPtr]::Zero)
      Start-Sleep -Milliseconds 35
    }
    Release-Focus
    "camera dx=$Dx dy=$Dy"
  }

  'zoom' {
    # Ticks negatif = reculer la camera. -120 ne rentre pas dans un uint32 :
    # il faut passer son equivalent non signe, sinon PowerShell leve.
    $h = (Get-Game).MainWindowHandle; Grab-Focus $h
    $n = [Math]::Abs($Ticks)
    if ($n -eq 0) { throw "-Ticks requis (negatif = reculer)" }
    $delta = if ($Ticks -lt 0) { [uint32]4294967176 } else { [uint32]120 }
    foreach ($i in 1..$n) {
      [Vx]::mouse_event(0x0800, 0, 0, $delta, [IntPtr]::Zero)
      Start-Sleep -Milliseconds 90
    }
    Release-Focus
    "zoom $Ticks crans"
  }

  'walk' {
    $h = (Get-Game).MainWindowHandle; Grab-Focus $h
    $name = if ($Value) { $Value } else { 'w' }
    $k = Get-Vk $name
    if ($k.Shift) { [Vx]::keybd_event(0x10, 0, 0, [IntPtr]::Zero) }
    [Vx]::keybd_event($k.Vk, 0, 0, [IntPtr]::Zero)
    Start-Sleep -Seconds $Seconds
    [Vx]::keybd_event($k.Vk, 0, 2, [IntPtr]::Zero)
    if ($k.Shift) { [Vx]::keybd_event(0x10, 0, 2, [IntPtr]::Zero) }
    Release-Focus
    "$name maintenue $Seconds s"
  }

  'state' {
    $p = Get-Game; $h = $p.MainWindowHandle; $c = Get-Client $h
    [PSCustomObject]@{
      PID           = $p.Id
      Repond        = $p.Responding
      CPU_s         = [math]::Round($p.CPU, 1)
      RAM_Mo        = [math]::Round($p.WorkingSet64 / 1MB)
      Client        = "$($c.W)x$($c.H)"
      OrigineEcran  = "$($c.SX),$($c.SY)"
      BasClient     = $c.SY + $c.H
      HauteurEcran  = [Vx]::GetSystemMetrics(1)
      Journal       = (Join-Path $OutDir 'game.out')
    } | Format-List
  }

  'stop' {
    $p = Get-Game
    # WM_CLOSE : la boucle winit le traite et quitte proprement.
    [void][Vx]::PostMessage($p.MainWindowHandle, 0x0010, [IntPtr]0, [IntPtr]0)
    foreach ($i in 1..20) {
      Start-Sleep -Milliseconds 500
      if (-not (Get-Process -Id $p.Id -ErrorAction SilentlyContinue)) { "arrete proprement"; return }
    }
    Stop-Process -Id $p.Id -Force
    "force apres 10 s"
  }
}
