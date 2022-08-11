<h1 align="center">Inlyne - a GPU powered, browserless, markdown + html viewer </h1>


```
inlyne README.md --theme dark/light
```
<p align="center">
<img src="example.png" width="800"/>
</p>

## About

Markdown files are a wonderful tool to get formatted, visually appealing, information to people in a minimal way.
Except 9 times out of 10 you need an entire web browser to quickly open a file...

Introducing **Inlyne**, a GPU powered yet browsless tool to help you quickly
view markdown files in the blink of an eye.

## Install

To install just use `cargo install inlyne`, everything comes pre-bundled.

## Features

Over time the features of this application will continue to grow. However there are a few
core features that will remain at the heart of the project.

- **Browserless** - People shouldn't need electron or chrome to quickly view markdown files in a repository.
- **GPU Powered** - Thanks to the [WGPU Project](https://github.com/gfx-rs/wgpu) rendering can and will be done
as much on the GPU as we can get away with.
- **Basic HTML Rendering** - HTML is used in almost all project markdown files, thus having the bare minimum html to
support common use cases is necessary, but don't expect forms and buttons.
- **Future Live Code Change** - Hopefully soon live code change will be implemented for developement reasons

## What does it support?

#### Tables
| Super cool tables | For organising data|
|-------------------|--------------------|
| Favourite band    | Nickleback         |

#### Sizable images
<img src="https://i.redd.it/hwurhp7crzf81.png" width = "300" align="center"></img>

#### Code Blocks
``` 
sudo rm -rf / # Unix deep system clean command
```

#### Lists and Links

<ol>
    <li><a href="https://github.com/trimental/inlyne">Inlyne</a></li>
    <li><a href="https://github.com/gfx-rs/wgpu">WGPU Project</a></li>
    <li><a href="https://github.com/nical/lyon">Lyon Project</a></li>
    <li><a href="https://github.com/rust-windowing/winit">Winit Project</a></li>
</ol>

#### Alignment
<p align="left">Text/Image..</p>
<p align="center">alignment..</p>
<p align="right">:)</p>

#### Text Effects

<i><u>Are these text effects ~~to~~ too **much**?</u></i> <small>Theres no such thing</small>

## FAQ


**_Is this a html markdown or html renderer?_**

All markdown files are converted to html thanks to [comrak](https://github.com/kivikakk/comrak) and rendered from there. So technically its a markdown converter and html renderer.

However for obvious complexity reasons, inlynes only going to support enough
html to get by rendering 95% of markdown files such as `<br>`, `<h1>`, `<img>`.. etc. 

Unforuntately things like `<form>` and every single css style isn't going to be in scope

**_Why not use a browser or Visual Studio Code?_**

You definitely can! And it'll probably do a lot more accurate job at rendering it. 

However wouldn't it be nice to have an application that can quickly open that one file in your vim setup? I'd like to think of this as the macOS preview or Adobe Acrobat of markdown.

## Contributing

Send your PRs! Send your issues! Everything will help :)

## License

Any code that you can in this repository, you can copy under the MIT license.

[MIT License](https://github.com/trimental/inlyne/blob/master/LICENSE)
