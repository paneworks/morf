/* SPDX-License-Identifier: MIT
 * GCR system prompter with a morf UI over private stdin/stdout pipes.
 * GCR handles D-Bus ownership of requests and encrypted secret exchange.
 */
#define GCR_API_SUBJECT_TO_CHANGE
#include <gcr/gcr.h>
#include <gio/gunixinputstream.h>
#include <glib-unix.h>
#include <gio/gunixsocketaddress.h>
#include <glib/gstdio.h>
#include <sys/stat.h>
#include <unistd.h>
#include <json-glib/json-glib.h>
#include <sys/resource.h>
#include <sys/prctl.h>
#include <string.h>
#include <stdio.h>

static GMainLoop *loop;
static GHashTable *pending;
static guint serial;
static gboolean disconnected;
static char *answer_path;
static const char *names[] = { NULL, "title", "message", "description", "warning",
    "choice-label", "choice-chosen", "password-new", "password-strength",
    "caller-window", "continue-label", "cancel-label" };
#define N_PROPS G_N_ELEMENTS(names)

typedef struct {
    GObject parent;
    GValue values[N_PROPS];
    GTask *task;
    char *secret;
    guint id;
    gulong cancel_id;
    gboolean password;
} MorfPrompt;
typedef struct { GObjectClass parent; } MorfPromptClass;
static void prompt_iface_init(GcrPromptInterface *iface);
G_DEFINE_TYPE_WITH_CODE(MorfPrompt, morf_prompt, G_TYPE_OBJECT,
                       G_IMPLEMENT_INTERFACE(GCR_TYPE_PROMPT, prompt_iface_init))

static void clear_secret(MorfPrompt *self) {
    if (self->secret) { explicit_bzero(self->secret, strlen(self->secret)); g_free(self->secret); self->secret=NULL; }
}
static void emit(JsonBuilder *builder) {
    if (!disconnected) {
        JsonGenerator *generator=json_generator_new();
        JsonNode *root=json_builder_get_root(builder);
        json_generator_set_root(generator,root);
        char *text=json_generator_to_data(generator,NULL);
        if (puts(text)==EOF || fflush(stdout)==EOF) { disconnected=TRUE; g_main_loop_quit(loop); }
        g_free(text); json_node_free(root); g_object_unref(generator);
    }
    g_object_unref(builder);
}
static JsonBuilder *event(const char *kind, guint id) {
    JsonBuilder *b=json_builder_new(); json_builder_begin_object(b);
    json_builder_set_member_name(b,"event"); json_builder_add_string_value(b,kind);
    if (id) { json_builder_set_member_name(b,"id"); json_builder_add_int_value(b,id); }
    return b;
}
static void complete(MorfPrompt *self, gboolean accepted, const char *secret, gboolean choice) {
    if (!self->task) return;
    GTask *task=g_steal_pointer(&self->task);
    GCancellable *cancel=g_task_get_cancellable(task);
    if (cancel && self->cancel_id) g_cancellable_disconnect(cancel,self->cancel_id);
    self->cancel_id=0;
    clear_secret(self);
    if (accepted && self->password) self->secret=g_strdup(secret ? secret : "");
    /* GcrPrompt strength is zero for empty passwords, positive otherwise. */
    g_value_set_int(&self->values[8],self->secret && *self->secret ? 1 : 0);
    g_object_notify(G_OBJECT(self),"password-strength");
    g_object_set(self,"choice-chosen",choice,NULL);
    g_hash_table_remove(pending,GUINT_TO_POINTER(self->id));
    JsonBuilder *b=event("close",self->id); json_builder_end_object(b); emit(b);
    g_task_return_boolean(task,accepted);
    g_object_unref(task);
}
static gboolean cancel_later(gpointer data) { complete(data,FALSE,NULL,FALSE); return G_SOURCE_REMOVE; }
static void cancelled(GCancellable *cancel, gpointer data) {
    (void)cancel;
    g_idle_add_full(G_PRIORITY_DEFAULT,cancel_later,g_object_ref(data),g_object_unref);
}
static void begin(MorfPrompt *self, gboolean password, GCancellable *cancel, GAsyncReadyCallback callback, gpointer data) {
    g_return_if_fail(self->task==NULL);
    clear_secret(self);
    self->id=++serial; self->password=password;
    self->task=g_task_new(self,cancel,callback,data);
    g_hash_table_insert(pending,GUINT_TO_POINTER(self->id),self);
    if (cancel) self->cancel_id=g_cancellable_connect(cancel,G_CALLBACK(cancelled),self,NULL);
    JsonBuilder *b=event("prompt",self->id);
    if (answer_path) { json_builder_set_member_name(b,"endpoint"); json_builder_add_string_value(b,answer_path); }
    json_builder_set_member_name(b,"kind"); json_builder_add_string_value(b,password ? "password" : "confirm");
    json_builder_set_member_name(b,"properties"); json_builder_begin_object(b);
    for (guint i=1;i<N_PROPS;i++) {
        if (!strcmp(names[i],"caller-window") || !strcmp(names[i],"password-strength")) continue;
        json_builder_set_member_name(b,names[i]);
        if (G_VALUE_HOLDS_STRING(&self->values[i])) {
            const char *value=g_value_get_string(&self->values[i]); json_builder_add_string_value(b,value ? value : "");
        } else json_builder_add_boolean_value(b,g_value_get_boolean(&self->values[i]));
    }
    json_builder_end_object(b); json_builder_end_object(b); emit(b);
}
static void password_async(GcrPrompt *prompt,GCancellable *cancel,GAsyncReadyCallback cb,gpointer data) {
    begin((MorfPrompt *)prompt,TRUE,cancel,cb,data);
}
static const char *password_finish(GcrPrompt *prompt,GAsyncResult *result,GError **error) {
    MorfPrompt *self=(MorfPrompt *)prompt;
    /* GCR borrows this pointer while encrypting it; keep it until next use. */
    return g_task_propagate_boolean(G_TASK(result),error) ? self->secret : NULL;
}
static void confirm_async(GcrPrompt *prompt,GCancellable *cancel,GAsyncReadyCallback cb,gpointer data) {
    begin((MorfPrompt *)prompt,FALSE,cancel,cb,data);
}
static GcrPromptReply confirm_finish(GcrPrompt *prompt,GAsyncResult *result,GError **error) {
    (void)prompt;
    return g_task_propagate_boolean(G_TASK(result),error) ? GCR_PROMPT_REPLY_CONTINUE : GCR_PROMPT_REPLY_CANCEL;
}
static void prompt_close(GcrPrompt *prompt) {
    complete((MorfPrompt *)prompt,FALSE,NULL,FALSE);
    clear_secret((MorfPrompt *)prompt);
}
static void prompt_iface_init(GcrPromptInterface *iface) {
    iface->prompt_password_async=password_async; iface->prompt_password_finish=password_finish;
    iface->prompt_confirm_async=confirm_async; iface->prompt_confirm_finish=confirm_finish;
    iface->prompt_close=prompt_close;
}
static void get_property(GObject *object,guint id,GValue *value,GParamSpec *spec) {
    if (id>0 && id<N_PROPS) g_value_copy(&((MorfPrompt *)object)->values[id],value);
    else G_OBJECT_WARN_INVALID_PROPERTY_ID(object,id,spec);
}
static void set_property(GObject *object,guint id,const GValue *value,GParamSpec *spec) {
    if (id>0 && id<N_PROPS) g_value_copy(value,&((MorfPrompt *)object)->values[id]);
    else G_OBJECT_WARN_INVALID_PROPERTY_ID(object,id,spec);
}
static void finalize(GObject *object) {
    MorfPrompt *self=(MorfPrompt *)object; clear_secret(self);
    for (guint i=1;i<N_PROPS;i++) g_value_unset(&self->values[i]);
    G_OBJECT_CLASS(morf_prompt_parent_class)->finalize(object);
}
static void morf_prompt_class_init(MorfPromptClass *klass) {
    GObjectClass *object=G_OBJECT_CLASS(klass);
    object->get_property=get_property; object->set_property=set_property; object->finalize=finalize;
    for (guint i=1;i<N_PROPS;i++) g_object_class_override_property(object,i,names[i]);
}
static void morf_prompt_init(MorfPrompt *self) {
    for (guint i=1;i<N_PROPS;i++) {
        GParamSpec *spec=g_object_class_find_property(G_OBJECT_GET_CLASS(self),names[i]);
        g_value_init(&self->values[i],G_PARAM_SPEC_VALUE_TYPE(spec)); g_param_value_set_default(spec,&self->values[i]);
    }
}
static const char *string_member(JsonObject *object,const char *name) {
    JsonNode *n=json_object_get_member(object,name);
    return n && JSON_NODE_HOLDS_VALUE(n) && json_node_get_value_type(n)==G_TYPE_STRING ? json_node_get_string(n) : NULL;
}
static gboolean boolean_member(JsonObject *object,const char *name) {
    JsonNode *n=json_object_get_member(object,name);
    return n && JSON_NODE_HOLDS_VALUE(n) && json_node_get_value_type(n)==G_TYPE_BOOLEAN && json_node_get_boolean(n);
}
static gboolean handle_line(const char *line,gsize length) {
    JsonParser *parser=json_parser_new();
    /* Do not log parse errors; malformed input could contain a secret. */
    if (!json_parser_load_from_data(parser,line,length,NULL)) goto invalid;
    JsonNode *root=json_parser_get_root(parser);
    if (!JSON_NODE_HOLDS_OBJECT(root)) goto invalid;
    JsonObject *object=json_node_get_object(root);
    JsonNode *id=json_object_get_member(object,"id");
    if (!id || !JSON_NODE_HOLDS_VALUE(id) || json_node_get_value_type(id)!=G_TYPE_INT64) goto invalid;
    gint64 value=json_node_get_int(id);
    if (value<=0 || value>G_MAXUINT) goto invalid;
    MorfPrompt *prompt=g_hash_table_lookup(pending,GUINT_TO_POINTER((guint)value));
    const char *action=string_member(object,"action"), *secret=string_member(object,"password");
    if (!action || (strcmp(action,"continue") && strcmp(action,"cancel"))) goto invalid;
    if (prompt) {
        gboolean accepted=action && !strcmp(action,"continue");
        if (prompt->password && accepted && (!secret || strlen(secret)>16384)) accepted=FALSE;
        complete(prompt,accepted,secret,boolean_member(object,"choice"));
    }
    if (secret) explicit_bzero((char *)secret,strlen(secret));
    g_object_unref(parser); return prompt!=NULL;
invalid:
    g_object_unref(parser); return FALSE;
}
static GByteArray *input;
static void read_done(GObject *stream,GAsyncResult *result,gpointer data) {
    (void)data;
    GError *error=NULL;
    GBytes *bytes=g_input_stream_read_bytes_finish(G_INPUT_STREAM(stream),result,&error);
    gsize length=0; const guint8 *chunk=bytes ? g_bytes_get_data(bytes,&length) : NULL;
    if (!chunk || !length || input->len+length>65536) { g_clear_error(&error); if (bytes) g_bytes_unref(bytes); g_main_loop_quit(loop); return; }
    g_byte_array_append(input,chunk,length); g_bytes_unref(bytes);
    guint8 *newline;
    while ((newline=memchr(input->data,'\n',input->len))) {
        guint consumed=(guint)(newline-input->data)+1;
        handle_line((char *)input->data,consumed-1);
        explicit_bzero(input->data,consumed); g_byte_array_remove_range(input,0,consumed);
    }
    g_input_stream_read_bytes_async(G_INPUT_STREAM(stream),4096,G_PRIORITY_DEFAULT,NULL,read_done,NULL);
}
/* Only answers cross this socket. Its random parent directory is private to
 * this user; the listener also checks the peer UID. Metadata stays on stdout.
 * Each connection has a byte limit and deadline, and answers are never logged. */
typedef struct {
    GSocketConnection *connection;
    GCancellable *cancel;
    GByteArray *bytes;
    guint timeout;
} Answer;
static guint answer_count;
static gboolean answer_timeout(gpointer data) {
    Answer *a=data; a->timeout=0; g_cancellable_cancel(a->cancel); return G_SOURCE_REMOVE;
}
static void answer_free(Answer *a) {
    answer_count--;
    if (a->timeout) g_source_remove(a->timeout);
    g_io_stream_close(G_IO_STREAM(a->connection),NULL,NULL);
    explicit_bzero(a->bytes->data,a->bytes->len); g_byte_array_unref(a->bytes);
    g_object_unref(a->cancel); g_object_unref(a->connection); g_free(a);
}
static void answer_read(GObject *stream,GAsyncResult *result,gpointer data) {
    Answer *a=data;
    GBytes *bytes=g_input_stream_read_bytes_finish(G_INPUT_STREAM(stream),result,NULL);
    gsize length=0; const guint8 *chunk=bytes ? g_bytes_get_data(bytes,&length) : NULL;
    if (!length || a->bytes->len+length>65536) {
        if (bytes) g_bytes_unref(bytes);
        answer_free(a); return;
    }
    g_byte_array_append(a->bytes,chunk,length); g_bytes_unref(bytes);
    guint8 *newline=memchr(a->bytes->data,'\n',a->bytes->len);
    if (newline) {
        gboolean ok=handle_line((char *)a->bytes->data,newline-a->bytes->data);
        /* Tiny reply; nonblocking so a non-reading client cannot stall GCR. */
        GSocket *socket=g_socket_connection_get_socket(a->connection);
        g_socket_set_blocking(socket,FALSE);
        const char *reply=ok ? "ok\n" : "closed\n";
        g_socket_send(socket,reply,strlen(reply),NULL,NULL);
        answer_free(a); return;
    }
    g_input_stream_read_bytes_async(G_INPUT_STREAM(stream),4096,G_PRIORITY_DEFAULT,a->cancel,answer_read,a);
}
static gboolean answer_incoming(GSocketService *service,GSocketConnection *connection,GObject *source,gpointer data) {
    (void)service; (void)source; (void)data;
    GCredentials *credentials=g_socket_get_credentials(g_socket_connection_get_socket(connection),NULL);
    gboolean allowed=credentials && g_credentials_get_unix_user(credentials,NULL)==getuid();
    g_clear_object(&credentials);
    if (!allowed || answer_count>=16) return FALSE;
    answer_count++;
    Answer *a=g_new0(Answer,1);
    a->connection=g_object_ref(connection); a->cancel=g_cancellable_new(); a->bytes=g_byte_array_new();
    a->timeout=g_timeout_add_seconds(5,answer_timeout,a);
    g_input_stream_read_bytes_async(g_io_stream_get_input_stream(G_IO_STREAM(connection)),4096,
        G_PRIORITY_DEFAULT,a->cancel,answer_read,a);
    return TRUE;
}
static GSocketService *answer_listen(char **directory) {
    char *template=g_build_filename(g_get_user_runtime_dir(),"morf-keyring-XXXXXX",NULL);
    if (!g_mkdtemp_full(template,0700)) { g_free(template); return NULL; }
    *directory=template; answer_path=g_build_filename(template,"answer",NULL);
    GSocketService *service=g_socket_service_new();
    GSocketAddress *address=g_unix_socket_address_new(answer_path);
    gboolean ok=g_socket_listener_add_address(G_SOCKET_LISTENER(service),address,G_SOCKET_TYPE_STREAM,
        G_SOCKET_PROTOCOL_DEFAULT,NULL,NULL,NULL);
    g_object_unref(address);
    if (!ok || g_chmod(answer_path,0600)!=0) { g_object_unref(service); return NULL; }
    g_signal_connect(service,"incoming",G_CALLBACK(answer_incoming),NULL);
    g_socket_service_start(service); return service;
}
static gboolean owned[2];
static void ownership(GDBusConnection *connection,const char *name,gpointer data,gboolean acquired) {
    (void)connection; (void)name;
    owned[GPOINTER_TO_UINT(data)]=acquired;
    JsonBuilder *b=event("status",0); json_builder_set_member_name(b,"names"); json_builder_begin_array(b);
    if (owned[0]) json_builder_add_string_value(b,"org.gnome.keyring.SystemPrompter");
    if (owned[1]) json_builder_add_string_value(b,"org.gnome.keyring.PrivatePrompter");
    json_builder_end_array(b); json_builder_end_object(b); emit(b);
}
static void acquired(GDBusConnection *c,const char *n,gpointer d) { ownership(c,n,d,TRUE); }
static void lost(GDBusConnection *c,const char *n,gpointer d) { ownership(c,n,d,FALSE); }
static gboolean stop(gpointer data) { (void)data; g_main_loop_quit(loop); return G_SOURCE_REMOVE; }
int main(void) {
    struct rlimit limit={0,0}; setrlimit(RLIMIT_CORE,&limit); prctl(PR_SET_DUMPABLE,0);
    signal(SIGPIPE,SIG_IGN);
    loop=g_main_loop_new(NULL,FALSE); pending=g_hash_table_new(g_direct_hash,g_direct_equal);
    input=g_byte_array_new();
    GError *error=NULL; GDBusConnection *connection=g_bus_get_sync(G_BUS_TYPE_SESSION,NULL,&error);
    if (!connection) { fputs("morf-keyring: session bus unavailable\n",stderr); g_clear_error(&error); return 1; }
    char *answer_directory=NULL;
    GSocketService *answers=answer_listen(&answer_directory);
    if (!answers) { fputs("morf-keyring: private answer socket unavailable\n",stderr); return 1; }
    GcrSystemPrompter *prompter=gcr_system_prompter_new(GCR_SYSTEM_PROMPTER_SINGLE,morf_prompt_get_type());
    gcr_system_prompter_register(prompter,connection);
    guint owners[2];
    const char *bus_names[]={"org.gnome.keyring.SystemPrompter","org.gnome.keyring.PrivatePrompter"};
    for (guint i=0;i<2;i++) owners[i]=g_bus_own_name_on_connection(connection,bus_names[i],G_BUS_NAME_OWNER_FLAGS_NONE,acquired,lost,GUINT_TO_POINTER(i),NULL);
    GInputStream *stream=g_unix_input_stream_new(0,FALSE);
    g_input_stream_read_bytes_async(stream,4096,G_PRIORITY_DEFAULT,NULL,read_done,NULL);
    g_unix_signal_add(SIGTERM,stop,NULL); g_unix_signal_add(SIGINT,stop,NULL);
    g_main_loop_run(loop);
    disconnected=TRUE;
    gcr_system_prompter_unregister(prompter,FALSE);
    for (guint i=0;i<2;i++) g_bus_unown_name(owners[i]);
    explicit_bzero(input->data,input->len); g_byte_array_unref(input);
    g_object_unref(stream); g_object_unref(prompter); g_object_unref(connection);
    g_socket_service_stop(answers); g_object_unref(answers);
    g_unlink(answer_path); g_rmdir(answer_directory); g_free(answer_path); g_free(answer_directory);
    g_hash_table_unref(pending); g_main_loop_unref(loop);
    return 0;
}
