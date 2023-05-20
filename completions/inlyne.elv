
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
            cand -s 'Factor to scale rendered file by [default: OS defined window scale factor]'
            cand --scale 'Factor to scale rendered file by [default: OS defined window scale factor]'
            cand -c 'Configuration file to use'
            cand --config 'Configuration file to use'
            cand -w 'Maximum width of page in pixels'
            cand --page-width 'Maximum width of page in pixels'
            cand -h 'Print help'
            cand --help 'Print help'
            cand -V 'Print version'
            cand --version 'Print version'
        }
    ]
    $completions[$command]
}
