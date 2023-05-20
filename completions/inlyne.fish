complete -c inlyne -s t -l theme -d 'Theme to use when rendering' -r -f -a "{auto	,dark	,light	}"
complete -c inlyne -s s -l scale -d 'Factor to scale rendered file by [default: OS defined window scale factor]' -r
complete -c inlyne -s c -l config -d 'Configuration file to use' -r -F
complete -c inlyne -s w -l page-width -d 'Maximum width of page in pixels' -r
complete -c inlyne -s h -l help -d 'Print help'
complete -c inlyne -s V -l version -d 'Print version'
