* Web: If I send a message and OpenCode isn't connected correctly, when it is connected the message is still just "thinking"
* Web: The formatting for the agent return messages ends up having a ton of surprising whitespace for some reason?
* Web: My message/prompt doesn't appear in the log?  Looks like the throbber just keeps spinning
* Web: I can't scroll up.  It just keeps scrolling me back to the bottom.
* Web: Clicking on "<- Journals" changes the URL but doesn't reload the home
* Web: When I type a message into the journal, it should be immediately visible locally, probably saved to local storage to be durable on restart, marked that it isn't yet synced and there should be an indication that we're trying to get it integrated into the durable journal log
* The UX of a journal should not be a chat as it is now, but that of a Jupyter Notebook with "prompts" being code blocks and the resulting agent thread being the output of a run
* The instigating "prompt blocks" should be multifaceted (probably have tabs on the side visually to switch view?) as they  should have the prompt explicitly put in by the  instigator, but also they should show the complete actual text sent to the LLM annotated by _why_ that text was included in the prompt and _where_ that text came from in terms of provenance
* Any and all information injected into the prompts behind the scenes should carry provenance information and when you're viewing the "raw" view of a prompt you should be able to click "through" to the originating message, etc.  Think of compaction and then the actions that follow, where did various pieces come from?
* Each block between prompt and instigated chain becomes a resource that can be introspected later.  We're generating a context tree that should be available to agents via the CLI such that compactions can reference entities in the journal as a first class thing and don't have to pass around half-misinterpreted summaries.
