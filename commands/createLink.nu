# createLink allows simlink in Windows easy
def createLink [
  src: any   # Path to link into target
  target: any   # Path where link src
  --dir (-D): int  # 0 for no dir other value directory
] {
    if (sys).host.name == "Windows" {
        do { 
            let target = ($target | str replace -a '/' '\')
            let src = ($src | str replace -a '/' '\')
            # println (echo "linking " $src " to " $target  | str collect)
            if  $dir != 0  {
                mklink  /D $target $src
            } else {
                mklink  $target $src
            }
        }
    } else {
        # println (echo "linking " $src " to " $target  | str collect)    
        ln -s $src $target 
    }
}