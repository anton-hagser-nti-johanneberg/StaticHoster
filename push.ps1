$version = $args[0];
$oldversion = $version.Replace('.', '')
$oldversion = $oldversion;
$oldversion -= 1;
$oldversion = "$($oldversion.ToString().ToCharArray()[0]).$($oldversion.ToString().ToCharArray()[1]).$($oldversion.ToString().ToCharArray()[2])"

Write-Output "New version: $version"
Write-Output "Last version: $oldversion"

Invoke-Expression "docker build -t static-host:$version ./"
Invoke-Expression "docker image save static-host:$version -o static-host.tar"
Invoke-Expression "scp ./static-host.tar root@ssh.hapsy.net:~/images/static-host.tar"
$sed = "sed -i 's/$oldversion/$version/g' ./manifests/school/deployment.static-host.yaml"
$command = "$sed; cd ./images; ./run.bat;";
Invoke-Expression "ssh root@hapsy.net '$command'"

((Get-Content -path ./Cargo.toml -Raw) -replace "$oldversion","$version") | Set-Content -Path ./Cargo.toml