# Overview:

This whole entire thing is designed to fill a need in an existing project I have. Namely its designed to be a:

"Collaborative rich text editor"

for the gui framework egui (https://docs.rs/egui/latest/egui/). 

As for the CRDT system, I think that using automerge (https://automerge.org/docs/hello/) is the best option, not only because its second in popularity in YJS and is written in rust, but also because it supports this insanely cool authentication and management solution in keyhive 
- vision doc: https://www.inkandswitch.com/keyhive/notebook/
- project gh: https://github.com/inkandswitch/keyhive


Another optional thing would be that the editor is to some extent markdown native. So the text essentially has 2 equal and equivalent states, one as Rich Text, and the other as markdown. So any human working on the document can see the entire thing as rich text and edit it exactly like they would in any other rich text editor. 

And at the same time any LLM can come up to the collaborative session and see a document in markdown, make modification to the markdown document, and those show up as rich text modifications on the frontend UI editor that the humans are using.

There are seemingly multiple ways of doing this. Which is essentially dependant on if you want your fundamental source of truth for the data to be a simple array/rope of markdown utf8 encoded bytes or a rich text tree that encodes the styling therein. And for the system that you do support you just use native text editing, but the other system you solve by having a bidirectional transform, and then transforming any edits. The milkdown editor takes the first approach of using markdown as the native format: (https://milkdown.dev/docs/guide/getting-started). But the other approach has merits, (its what I used on my last project), what would your thoughts be on all this? Is there anything you would want to look up before continuing the architecture discussion. 
