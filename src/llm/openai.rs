use std::fmt::{self, Display, Formatter};

use super::request::RequestMessages;
use crate::llm::LLM;
use anyhow::Result;
pub use async_openai::config::{Config, OpenAIConfig};
use async_openai::types::{CreateChatCompletionRequest, CreateChatCompletionRequestArgs, Role as R};
use serde::Serialize;

use super::LLMResponse;

#[derive(Serialize, Debug, Clone)]
pub struct Role(R);

impl From<&str> for Role {
    fn from(role: &str) -> Self {
        match role {
            "system" => Role(R::System),
            "user" => Role(R::User),
            "assistant" => Role(R::Assistant),
            "function" => Role(R::Function),
            _ => Role(R::System),
        }
    }
}

impl Display for Role {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        match self.0 {
            R::System => write!(f, "system"),
            R::User => write!(f, "user"),
            R::Assistant => write!(f, "assistant"),
            R::Function => write!(f, "function"),
        }
    }
}

#[derive(Serialize, Debug, Clone)]
pub struct Message {
    /// The message role (system, user, assistant)
    pub role: Role,

    /// The message text
    pub message: String,
}

impl Message {
    pub fn new(role: Role, message: &str) -> Message {
        Message {
            role,
            message: message.to_string(),
        }
    }

    pub fn into_vec(v: Vec<(&str, &str)>) -> Vec<Message> {
        let mut messages = Vec::new();
        for (role, message) in v {
            messages.push(Message::new(role.into(), message));
        }
        messages
    }
}

impl Display for Message {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        write!(f, "[{}] {}", self.role, self.message)
    }
}

pub struct OpenAIClient {
    /// Client member for the OpenAI API. This client is a wrapper around the async-openai crate, with additional functionality to
    /// support LLM orchestration.
    client: async_openai::Client<OpenAIConfig>,

    /// The prompt to use for the OpenAI API
    prompt: Option<Vec<Message>>,

    /// ID of the model to use.
    /// See the [model endpoint compatibility](https://platform.openai.com/docs/models/model-endpoint-compatibility) table for details on which models work with the Chat API.
    model: String,

    /// What sampling temperature to use, between 0 and 2. Higher values like 0.8 will make the output more random,
    /// while lower values like 0.2 will make it more focused and deterministic.
    ///
    /// We generally recommend altering this or `top_p` but not both.
    temperature: f32, // min: 0, max: 2, default: 1,

    /// An alternative to sampling with temperature, called nucleus sampling,
    /// where the model considers the results of the tokens with top_p probability mass.
    /// So 0.1 means only the tokens comprising the top 10% probability mass are considered.
    ///
    ///  We generally recommend altering this or `temperature` but not both.
    top_p: f32, // min: 0, max: 1, default: 1

    /// If set, partial message deltas will be sent, like in ChatGPT.
    /// Tokens will be sent as data-only [server-sent events](https://developer.mozilla.org/en-US/docs/Web/API/Server-sent_events/Using_server-sent_events#Event_stream_format) as they become available,
    /// with the stream terminated by a `data: [DONE]` message.[Example Python code](https://github.com/openai/openai-cookbook/blob/main/examples/How_to_stream_completions.ipynb).
    stream: bool,

    /// The maximum number of [tokens](https://platform.openai.com/tokenizer) to generate in the chat completion.
    ///
    /// The total length of input tokens and generated tokens is limited by the model's context length. [Example Python code](https://github.com/openai/openai-cookbook/blob/main/examples/How_to_count_tokens_with_tiktoken.ipynb) for counting tokens.
    max_tokens: u16,
}

impl Default for OpenAIClient {
    fn default() -> Self {
        Self {
            client: async_openai::Client::new(),
            prompt: None,
            model: "gpt-3.5-turbo".to_string(),
            temperature: 1.0,
            top_p: 1.0,
            stream: false,
            max_tokens: 1024u16,
        }
    }
}

impl OpenAIClient {
    /// Create a new OpenAI client
    pub fn new() -> Self {
        Self::default()
    }

    /// Set model to use
    /// e.g. "davinci", "gpt-3.5-turbo"
    pub fn with_model(mut self, model: &str) -> Self {
        self.model = model.to_string();
        self
    }

    /// Set prompt to use
    /// e.g. "What is the capital of France?"
    /// This is the prompt that will be used to generate the response.
    pub fn with_prompt(mut self, prompt: Vec<Message>) -> Self {
        self.prompt = Some(prompt);
        self
    }

    /// What sampling temperature to use, between 0 and 2. Higher values like 0.8 will make the output more random,
    /// while lower values like 0.2 will make it more focused and deterministic.
    pub fn with_temperature(mut self, temperature: f32) -> Self {
        self.temperature = temperature;
        self
    }

    /// An alternative to sampling with temperature, called nucleus sampling,
    /// where the model considers the results of the tokens with top_p probability mass.
    pub fn with_top_p(mut self, top_p: f32) -> Self {
        self.top_p = top_p;
        self
    }

    /// If set, partial message deltas will be sent, like in ChatGPT.
    pub fn with_stream(mut self, stream: bool) -> Self {
        self.stream = stream;
        self
    }

    /// The maximum number of [tokens](https://platform.openai.com/tokenizer) to generate in the chat completion.
    pub fn with_max_tokens(mut self, max_tokens: u16) -> Self {
        self.max_tokens = max_tokens;
        self
    }

    /// Generate a request for the OpenAI API and set the parameters
    pub fn generate_request(&self, messages: &[Message]) -> Result<CreateChatCompletionRequest> {
        Ok(CreateChatCompletionRequestArgs::default()
            .model(self.model.clone())
            .max_tokens(self.max_tokens)
            .temperature(self.temperature)
            .top_p(self.top_p)
            .stream(self.stream)
            .messages(RequestMessages::from(messages.to_owned()))
            .build()?)
    }
}

// Now implement these traits for your LLM types
#[async_trait::async_trait(?Send)]
impl LLM for OpenAIClient {
    async fn generate(&self) -> Result<LLMResponse> {
        let request = self.generate_request(&self.prompt.unwrap())?;

        let res = self.client.chat().create(request).await?;
        Ok(res.into())
    }
}

#[cfg(test)]
mod test {
    use super::*;
    use crate::prompt::context::Context;
    use crate::prompt::PromptEngine;
    use crate::prompts;

    #[tokio::test]
    async fn test_generate() {
        let client = OpenAIClient::new();
        let mut context = Context::new();
        context.set("country1", "France");
        context.set("country2", "Germany");
        let prompt = prompts!(
            ("user", "What is the capital of {{country1}}"),
            ("ai", "Paris"),
            ("user", "What is the capital of {{country2}}")
        );
        let prompt = prompt.render_context(&context).unwrap();
        let response = client.generate(&prompt).await.unwrap();
        assert!(response.get_response_content().to_lowercase().contains("berlin"));
    }
}
