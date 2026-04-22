
use builtin;
use str;

set edit:completion:arg-completer[inlyne] = {|@words|
    fn spaces {|n|
        builtin:repeat $n ' ' | str:join ''
    }
    fn cand {|text desc|
        edit:complex-candidate $text &display=$text' '(spaces (- 14 (wcswidth $text)))$desc
    }
    var command = 'inlyne'
    for word $words[1..-1] {
        if (str:has-prefix $word '-') {
            break
        }
        set command = $command';'$word
    }
    var completions = [
        &'inlyne'= {
            cand -t 'Theme to use when rendering'
            cand --theme 'Theme to use when rendering'
            cand -d 'Enable decorations'
            cand --decorations 'Enable decorations'
            cand -s 'Factor to scale rendered file by [default: OS defined window scale factor]'
            cand --scale 'Factor to scale rendered file by [default: OS defined window scale factor]'
            cand -c 'Configuration file to use'
            cand --config 'Configuration file to use'
            cand -w 'Maximum width of page in pixels'
            cand --page-width 'Maximum width of page in pixels'
            cand -p 'Position of the opened window <x>,<y>'
            cand --win-pos 'Position of the opened window <x>,<y>'
            cand --win-size 'Size of the opened window <width>x<height>'
            cand --font-size 'Font size [default: 16]'
            cand --line-height 'Line height [default: 1.1]'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
            cand view 'View a markdown file with inlyne'
            cand config 'Configuration related things'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'inlyne;view'= {
            cand -t 'Theme to use when rendering'
            cand --theme 'Theme to use when rendering'
            cand -d 'Enable decorations'
            cand --decorations 'Enable decorations'
            cand -s 'Factor to scale rendered file by [default: OS defined window scale factor]'
            cand --scale 'Factor to scale rendered file by [default: OS defined window scale factor]'
            cand -c 'Configuration file to use'
            cand --config 'Configuration file to use'
            cand -w 'Maximum width of page in pixels'
            cand --page-width 'Maximum width of page in pixels'
            cand -p 'Position of the opened window <x>,<y>'
            cand --win-pos 'Position of the opened window <x>,<y>'
            cand --win-size 'Size of the opened window <width>x<height>'
            cand --font-size 'Font size [default: 16]'
            cand --line-height 'Line height [default: 1.1]'
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'inlyne;config'= {
            cand -h 'Print help'
            cand --help 'Print help'
            cand open 'Opens the configuration file in the default text editor'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'inlyne;config;open'= {
            cand -h 'Print help'
            cand --help 'Print help'
        }
        &'inlyne;config;help'= {
            cand open 'Opens the configuration file in the default text editor'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'inlyne;config;help;open'= {
        }
        &'inlyne;config;help;help'= {
        }
        &'inlyne;help'= {
            cand view 'View a markdown file with inlyne'
            cand config 'Configuration related things'
            cand help 'Print this message or the help of the given subcommand(s)'
        }
        &'inlyne;help;view'= {
        }
        &'inlyne;help;config'= {
            cand open 'Opens the configuration file in the default text editor'
        }
        &'inlyne;help;config;open'= {
        }
        &'inlyne;help;help'= {
        }
    ]
    $completions[$command]
}
