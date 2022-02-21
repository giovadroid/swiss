
# First at all is to install OpenSSH Server

Add-WindowsCapability -Online -Name OpenSSH.Server*
Set-ExecutionPolicy RemoteSigned -scope CurrentUser

Invoke-Expression (New-Object System.Net.WebClient).DownloadString('https://get.scoop.sh')
#Invoke-RestMethod -Uri "https://aka.ms/vs/17/release/vs_BuildTools.exe" -OutFile vs_BuildTools.exe

$url = "https://aka.ms/vs/16/release/vs_BuildTools.exe";
    # https://visualstudio.microsoft.com/es/thank-you-downloading-visual-studio/?sku=BuildTools&rel=16

#$url = 'https://download.microsoft.com/download/5/A/8/5A8B8314-CA70-4225-9AF0-9E957C9771F7/vs_BuildTools.exe';

$options = @(
    '--quiet',
    '--norestart',
    '--wait',
    '--add', 'Microsoft.VisualStudio.Workload.VCTools', '--includeOptional'
);

Install-FromExe -Name 'VSBuildTools2017' -Url $url -Options $options -NoVerify;


Invoke-RestMethod -Uri "https://static.rust-lang.org/rustup/dist/x86_64-pc-windows-msvc/rustup-init.exe" -OutFile rustup-init.exe
#.\vs_BuildTools.exe--quiet --add Microsoft.VisualStudio.Workload.VCTools
.\rustup-init.exe -y

scoop install starship nodejs-lts

cargo install nu --features=extra

nu -c "mklink /D ~/.swiss (pwd)"
nu script/installer/install.nu

# Start the sshd service
Start-Service sshd

# OPTIONAL but recommended:
Set-Service -Name sshd -StartupType 'Automatic'

# Confirm the Firewall rule is configured. It should be created automatically by setup. Run the following to verify
if (!(Get-NetFirewallRule -Name "OpenSSH-Server-In-TCP" -ErrorAction SilentlyContinue | Select-Object Name, Enabled)) {
        Write-Output "Firewall Rule 'OpenSSH-Server-In-TCP' does not exist, creating it..."
            New-NetFirewallRule -Name 'OpenSSH-Server-In-TCP' -DisplayName 'OpenSSH Server (sshd)' -Enabled True -Direction Inbound -Protocol TCP -Action Allow -LocalPort 22
            } else {
                    Write-Output "Firewall rule 'OpenSSH-Server-In-TCP' has been created and exists."
                        }

# New-ItemProperty -Path "HKLM:\SOFTWARE\OpenSSH" -Name DefaultShell -Value $env:UserProfile + "/.cargo/bin/nu" -PropertyType String -Force